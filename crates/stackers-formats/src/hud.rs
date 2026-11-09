//! The solo-mode screen around the field: background, window frame, next-pair window and the
//! HUD panels. Sprite records are read from the executable; positions are the constants the
//! draw code passes (see "Screen layout" in `docs/gameplay.md`).
//!
//! All screen sprites go through `func_8001B148` / `func_8001C7A8` / `func_8001B860`: a frame
//! record (see [`Frame`]) drawn at `(x + dx·s, y + dy·s)` with size `(w + (s ≠ 128))·s`, where
//! `s` is a scale out of 128. `page` indexes the uploaded-TIM slots: tpage `D_80066278[page]`,
//! CLUT `D_80103250[page][clut]`.

use crate::anm::Frame;
use crate::{exe::Exe, Error};

/// W_ASA frame and next window, drawn at (0, 0) by `func_8001A7BC` (OT layer 1, over the field).
const WINDOW: u32 = 0x8005_6820;
/// Panel records `D_8005666C[i]`: 0 = monitor, 1–6 labels, 7–9 menu words, 10 label (COMMON);
/// 11–14 LEVEL2; 15 "Pause" header, 16 quit confirmation (COMMON); 17–20 pause legends (MSG2).
const PANELS: u32 = 0x8005_666C;
const PANEL_COUNT: usize = 21;
/// ST22B patch below the next window, drawn with slot 0 of the queue (`func_80023294`).
const QUEUE_PATCH: u32 = 0x8005_5F14;

/// Digit fonts of `func_8001D4D8` (PSFONT, page 12), by its `kind` argument; digit `d` is
/// record `d`. The glyph advance is the width of digit 0.
const DIGIT_FONTS: [u32; 4] = [
    0x8005_643C, // other kinds: 16×16
    0x8005_64F0, // 1: 8×16
    0x8005_65B8, // 2: 8×8
    0x8005_6388, // 3: 16×32
];
/// `func_8001D29C` text font (its last argument ≠ 0): a space advances by record 0's width.
const TEXT_FONT: u32 = 0x8005_6844;
/// `func_8001D29C` escapes "kH" and "kS" (jump table `0x800103EC`): the hour and minute marks.
const GLYPH_H: u32 = 0x8005_6374;
const GLYPH_S: u32 = 0x8005_6350;
/// "k-" (escape table entry for '-').
const GLYPH_DASH: u32 = 0x8005_62F6;
/// "kD", the seconds mark (Time Trial clock).
const GLYPH_D: u32 = 0x8005_6362;
/// OJAWAKU2's garbage-field frame (page 21, 112×240), `func_8001A70C`.
const GARBAGE_FRAME: u32 = 0x8005_680C;
/// The in-game course list's arrow `D_80056A88[27]` (LEVEL2, page 17), `func_8002C16C`.
const COURSE_ARROWS: u32 = 0x8005_6A88;
const COURSE_ARROW_FRAMES: u32 = 27;

/// Screen = field pixel + this (`func_800195FC`: x + 8, y − camera − 80 with camera −16).
pub const FIELD_ORIGIN: (i32, i32) = (8, -64);

/// Queue (`func_80023294`): x = `D_800B0F90` (0x93 in solo, `func_80017480`).
pub const QUEUE_X: i32 = 0x93;
/// A queued pair is drawn at its slot y (`stackers_core::Game::queue_y`) + this.
pub const QUEUE_TOP: i32 = 0x2C;
/// Slot scale = max(128 − y, this).
pub const QUEUE_MIN_SCALE: i32 = 0x5B;
pub const QUEUE_PATCH_AT: (i32, i32) = (0x80, 0x84);
/// `func_8001B860` brightness for the patch (128 = unchanged).
pub const QUEUE_PATCH_BRIGHTNESS: i32 = 0x60;

/// HUD panels (`func_8001A560`): label then monitor at (200, y), both with the panel's palette.
pub const PANEL_X: i32 = 0xC8;
pub const PANEL_Y: [i32; 3] = [0x10, 0x58, 0xA0];
/// Arguments of the solo call in `func_8001A7BC`: palettes (1, 2, 4), labels (1, 2, 6) =
/// "Erase Number", "Record", "Course Time"; the monitors are semi-transparent.
pub const SOLO_PANEL_CLUTS: [i16; 3] = [1, 2, 4];
pub const SOLO_PANEL_LABELS: [usize; 3] = [1, 2, 6];

/// Pause menu (`func_8002DE14`, all `func_8001B860` at scale 128, OT layer 0), in the order
/// the original adds them, so each one is drawn over the ones after it:
/// `(record, x, y)`. Items while choosing (menu state ≠ 2, 3): Continue, Quit, Retry, legend.
pub const PAUSE_ITEMS: [(usize, i32, i32); 4] = [(7, 0x78, 0x60), (8, 0x78, 0x70), (9, 0x78, 0x80), (20, 0xE2, 0xD4)];
/// Shown instead of [`PAUSE_ITEMS`] while confirming or leaving through Quit.
pub const PAUSE_CONFIRM: (usize, i32, i32) = (16, 0x78, 0x60);
/// Always drawn after the items: header, its monitor, the three button legends.
pub const PAUSE_FRAME: [(usize, i32, i32); 5] = [(15, 0x80, 0x50), (0, 0x70, 0x50), (17, 0x32, 0xB4), (18, 0x8A, 0xB4), (19, 0xD2, 0xB4)];
/// Palettes `D_80103658` (header and monitor), `D_80103660` (confirmation), `D_80103662`
/// (legends), set by `func_8002DD80`.
pub const PAUSE_HEADER_CLUT: i16 = 4;
pub const PAUSE_CONFIRM_CLUT: i16 = 1;
pub const PAUSE_LEGEND_CLUT: i16 = 6;

/// Score (`func_8001DF84` in mode 4): 8 digits of font 1, palette 1, leading zeros shown.
pub const SCORE_AT: (i32, i32) = (0x84, 0xD0);
pub const SCORE_DIGITS: usize = 8;

/// One of the two counters in the top panels (`func_8002CC90`): "Erase Number" (`y` = 0x28,
/// small y 0x30) and "Record" (0x70 / 0x78). Returns `(font, digits, x, y, clut, zeros)`, or
/// `None` above 99,999,999.
pub fn counter_style(value: u32, record: bool) -> Option<(usize, usize, i32, i32, i16, bool)> {
    let (big_y, small_y) = if record { (0x70, 0x78) } else { (0x28, 0x30) };
    match value {
        0..=9_999 => Some((3, 4, 0xD8, big_y, 0, false)),
        10_000..=999_999 => Some((1, 6, 0xE0, small_y, 1, true)),
        1_000_000..=99_999_999 => Some((1, 8, 0xD8, small_y, 1, true)),
        _ => None,
    }
}

/// Course time (`func_8003DCB8`): " " at this point, then hours (no leading zero), "kH",
/// minutes, "kS", seconds, all 2 digits of font 1 with palette 1.
pub const TIME_AT: (i32, i32) = (0xD0, 0xC4);
pub const HUD_CLUT: i16 = 1;

/// Time Trial (mode 11, `func_8001A70C`): the frame `0x80056820` (W_ZERO on page 24) at (0, y)
/// where y is the shake offset of `func_8001A8B8` (0 unless garbage lands on player 1, which
/// never happens here), one monitor at (0xC8, 0xA0) with palette 2 (semi-transparent, layer 2),
/// and the garbage field's frame at (0xC0, 0) on layer 2.
pub const TRIAL_MONITOR_AT: (i32, i32) = (0xC8, 0xA0);
pub const TRIAL_MONITOR_CLUT: i16 = 2;
pub const GARBAGE_FRAME_AT: (i32, i32) = (0xC0, 0);
/// Player 2's field: `func_800195FC(2)` adds 0xB8 instead of 8 to x.
pub const P2_FIELD_DX: i32 = 0xB0;
/// Clock HUD `func_8003D9B0(clock, best)`: " " then minutes (2 digits, zeros), "kS", seconds,
/// "kD", then frames / 6 and (frames % 6)·2 capped at 9 (1 digit each), all font 1. The clock
/// at (0xD0, 0xB4) palette 7, the best at (0xD0, 0xCC) palette 1.
pub const TRIAL_CLOCK_AT: (i32, i32) = (0xD0, 0xB4);
pub const TRIAL_CLOCK_CLUT: i16 = 7;
pub const TRIAL_BEST_AT: (i32, i32) = (0xD0, 0xCC);
pub const TRIAL_BEST_CLUT: i16 = 1;

/// The digits `func_8003D9B0` shows for a packed clock: minutes, seconds, tenths-ish
/// (frames / 6) and the last digit ((frames % 6)·2, capped at 9).
pub fn trial_clock_digits(t: u32) -> (u32, u32, u32, u32) {
    let frames = t & 0x3F;
    ((t >> 16) & 0x3F, (t >> 8) & 0x3F, frames / 6, ((frames % 6) * 2).min(9))
}

/// `func_8001D4D8`'s digit slots, most significant first: `None` = a suppressed leading zero
/// (the pen still advances). The last digit is always drawn; `zeros` draws leading zeros.
pub fn digit_slots(mut value: u32, count: usize, zeros: bool) -> Vec<Option<u8>> {
    let mut d = vec![0u8; count];
    for slot in d.iter_mut().rev() {
        *slot = (value % 10) as u8;
        value /= 10;
    }
    let mut show = zeros;
    d.iter()
        .enumerate()
        .map(|(i, &v)| {
            show |= v != 0 || i == count - 1;
            show.then_some(v)
        })
        .collect()
}

/// Texture pages used by the solo screen and the disc file uploaded into each (solo-mode
/// loaders `func_80017744`, `func_800183C4`, `func_80018464`). Page 18 is the stage's 8-bit
/// background copy; [`background`] gives the matching full-screen image.
pub fn page_file(page: i16, stage: Stage) -> Option<String> {
    Some(match page {
        0..=9 => format!("DERODERO/GRAPH/DERO/DERO{page}.TIM"),
        12 => "DERODERO/GRAPH/PSFONT.TIM".into(),
        13 => "DERODERO/GRAPH/SOLO/COMMON.TIM".into(),
        18 => format!("DERODERO/GRAPH/SOLO/{}B.TIM", stage.name()),
        20 => "DERODERO/GRAPH/VS/VS_WIN2.TIM".into(),
        22 => "DERODERO/GRAPH/MSG2.TIM".into(),
        24 => "DERODERO/GRAPH/SOLO/W_ASA.TIM".into(),
        _ => return None,
    })
}

/// Pages Time Trial loads over the solo ones (`func_80018518`, `func_800185D4`): LEVEL2 = 17
/// (the in-game course list), ST53B = 18, OJAWAKU2 = 21, W_ZERO = 24. The background is
/// [`TRIAL_BACKGROUND`].
pub const TRIAL_PAGES: [i16; 4] = [17, 18, 21, 24];

pub fn trial_page_file(page: i16) -> Option<&'static str> {
    Some(match page {
        17 => "DERODERO/GRAPH/VS/LEVEL2.TIM",
        18 => "DERODERO/GRAPH/ZERO/ST53B.TIM",
        21 => "DERODERO/GRAPH/ZERO/OJAWAKU2.TIM",
        24 => "DERODERO/GRAPH/ZERO/W_ZERO.TIM",
        _ => return None,
    })
}

/// Pages the two-player modes load over the solo ones (`func_80017B20`): TAISEN_A = 18
/// (file 0x58; RANNY1_A / RANNY2_A in other set-ups), VS_WIN2 = 20, W_TAMA = 24; and
/// (`func_80018E68`) READYGO = 27 (until the VM uploads an SD image there), RENSA1/2 = 28/29.
/// Mode 9 (`func_80017DDC`, before every game mode) leaves RULET in 21 and TEAFONT in 25.
pub const VS_PAGES: [i16; 8] = [18, 20, 21, 24, 25, 27, 28, 29];
/// Every page [`vs_page_file`] knows, including the SD images.
pub const VS_FILES: [i16; 11] = [18, 20, 21, 24, 25, 27, 28, 29, 41, 42, 43];

/// The [`vs_page_file`] slot behind SD page 26/27 holding image `n` (1–3, `SdStage::vnao`).
pub fn vs_sd_page(n: u8) -> i16 {
    40 + i16::from(n)
}

pub fn vs_page_file(page: i16) -> Option<&'static str> {
    Some(match page {
        18 => "DERODERO/GRAPH/VS/TAISEN_A.TIM",
        20 => "DERODERO/GRAPH/VS/VS_WIN2.TIM",
        // `func_80017DDC`: files 4 and 0x53.
        21 => "DERODERO/GRAPH/SLOT/RULET.TIM",
        24 => "DERODERO/GRAPH/VS/LESSON/W_TAMA.TIM",
        25 => "DERODERO/GRAPH/VS/TEAFONT.TIM",
        // `func_80018E68` (start of `func_80018ADC`): files 10, 8, 9.
        27 => "DERODERO/GRAPH/READYGO.TIM",
        28 => "DERODERO/GRAPH/RENSA1.TIM",
        29 => "DERODERO/GRAPH/RENSA2.TIM",
        // The SD scripts' images (stage 0, `func_80018ADC`: files 0x89–0x8B), uploaded to
        // page 26 or 27 by the VM (see `SdStage::vnao`): [`vs_sd_page`].
        41 => "DERODERO/GRAPH/SD_TIM/VAMIGO13.TIM",
        42 => "DERODERO/GRAPH/SD_TIM/VAMIGO23.TIM",
        43 => "DERODERO/GRAPH/SD_TIM/VAMIGO33.TIM",
        _ => return None,
    })
}

/// The set match's full-screen background (file 0x55, `0x80138000`, when `D_800B137C` = 0).
pub const VS_BACKGROUND: &str = "DERODERO/GRAPH/VS/ST18A.TIM";
/// `func_8001A82C`: frame `0x80056820` record 0 at (0, y) and record 1 mirrored at (0xBF, y),
/// layer 1 (y = the shake of `func_8001A8B8`).
pub const VS_WINDOW_RIGHT_X: i32 = 0xBF;
/// `D_800B0F90/2` (`func_80016B18`): each player's queue x.
pub const VS_QUEUE_X: [i32; 2] = [0x88, 0xA8];
/// `func_8001E020`, mode 14: `func_8001D778(0x80, 0xB0, 8)`, the bomb stocks.
pub const VS_STOCK_X: [i32; 2] = [0x80, 0xB0];
pub const VS_STOCK_Y: i32 = 8;
/// `func_8001DBC0`: sets (1 digit, font 1, palette 5) at (0x78, 0x80), then "SET" at 0x84 and
/// "MATCH" at 0xA0 in the text font, palette 5.
pub const VS_SETS_AT: (i32, i32) = (0x78, 0x80);
pub const VS_SET_LABEL_X: i32 = 0x84;
pub const VS_MATCH_LABEL_X: i32 = 0xA0;
pub const VS_SETS_CLUT: i16 = 5;
/// `func_8001DC44`: wins (1 digit, font 1, palette 3) at (0x88, 0x90) and (0xB0, 0x90), "k-"
/// (palette 2) at 0x98 between them.
pub const VS_WINS_X: [i32; 2] = [0x88, 0xB0];
pub const VS_WINS_Y: i32 = 0x90;
pub const VS_DASH_X: i32 = 0x98;
pub const VS_WINS_CLUT: i16 = 3;
pub const VS_DASH_CLUT: i16 = 2;
/// `func_8001DCD8` (mode 14): match tallies at y 0xB0, palette 3, leading zeros. 1–3 digits by
/// size, right-aligned: player 1 at 0x8C / 0x88 / 0x84, player 2 at 0xAC / 0xA8 / 0xA4.
pub const VS_TALLY_Y: i32 = 0xB0;

/// The two-player strip over the fields (`func_8003C7C4`; frame records built by
/// `func_8003B958`, all on RULET, page 21). Offsets in `func_8001C03C` calls are from the screen
/// centre; see [`centred`].
pub mod strip {
    use crate::anm::Frame;

    const fn rulet(u: i16, v: i16, w: i16, h: i16) -> Frame {
        Frame { u, v, w, h, dx: 0, dy: 0, page: 21, clut: 21, flip: 0 }
    }

    /// `D_800684E8 + 0x12·p`: the bar behind the six slots, at ([`BAR_X`], [`ROW_Y`]),
    /// palette 1.
    pub const BAR: Frame = rulet(0x20, 0, 0x70, 0x20);
    pub const BAR_X: [i32; 2] = [-0x55, 0x5B];
    pub const BAR_CLUT: i16 = 1;
    /// `D_8010AC60`: one column's slot, at (`SLOT_X[p]` + 16·c, [`ROW_Y`]), palette 6.
    pub const SLOT: Frame = rulet(0, 0x48, 0x10, 0x18);
    pub const SLOT_X: [i32; 2] = [-0x80, 0x30];
    pub const SLOT_CLUT: i16 = 6;
    /// Bar and slots: y offset, and the vertical scale (horizontal is 0x80).
    pub const ROW_Y: i32 = -0x61;
    pub const ROW_SCALE: i32 = 0x56;
    /// `D_800C3120 + 0x30·p`: the special-column arrow, at (`COLUMN_X[p][column]` − 0x90,
    /// `ARROW_Y`), vertical scale [`ROW_SCALE`], palette `Roulette::clut`.
    pub const ARROW: Frame = rulet(0, 0, 0x20, 0x28);
    pub const ARROW_Y: i32 = -0x5D;
    /// `D_800557C4[p][c]` (x, y = 0): the arrow over column c + 1. Player 2's last two are 4 px
    /// short of the slots (252 and 268, not 256 and 272).
    pub const COLUMN_X: [[i32; 6]; 2] = [[16, 32, 48, 64, 80, 96], [192, 208, 224, 240, 252, 268]];
    pub const COLUMN_DX: i32 = -0x90;
    /// The sparkle (`D_80100394`, script frames on DERO pages): at the arrow's x + the wobble,
    /// y [`SPARKLE_Y`], scaled 0x56 both ways, palette 6.
    pub const SPARKLE_Y: i32 = -0x60;
    pub const SPARKLE_SCALE: i32 = 0x56;
    pub const SPARKLE_CLUT: i16 = 6;
    /// The garbage digits (`D_801036B4`, `func_8001B4D0`, unscaled): at (`DIGIT_X[p]` + 16·c,
    /// [`DIGIT_Y`]), palette `ColumnTally::clut`.
    pub const DIGIT_X: [i32; 2] = [0x18, 0xC8];
    pub const DIGIT_Y: i32 = 0xF;
    /// The message while a player's indicator spins (`D_800C2F9C`, only in play and pause):
    /// at (`MESSAGE_X[p]`, [`MESSAGE_Y`]), palette 1.
    pub const MESSAGE_X: [i32; 2] = [0x18, 0xC8];
    pub const MESSAGE_Y: i32 = 0x1E;
    pub const MESSAGE_CLUT: i16 = 1;

    /// `func_8001C03C`'s rectangle `(x, y, w, h)` for a `w`×`h` image at offset `(x, y)` from
    /// the screen centre, scaled `sx`/`sy` out of 128.
    pub fn centred(w: i32, h: i32, x: i32, y: i32, sx: i32, sy: i32) -> (i32, i32, i32, i32) {
        let x0 = 160 + x - (w * (sx + 7) / 128) / 2;
        let y0 = 120 + y - (h * (sy + 7) / 128) / 2;
        (x0, y0, (w * sx) >> 7, (h * sy) >> 7)
    }
}

/// `(digits, x)` of a match tally.
pub fn vs_tally_at(player: usize, value: u32) -> (usize, i32) {
    let base = [0x8C, 0xAC][player];
    match value {
        0..=9 => (1, base),
        10..=99 => (2, base - 4),
        _ => (3, base - 8),
    }
}

/// Time Trial's full-screen background (file 0x19, loaded to `0x80138000`).
pub const TRIAL_BACKGROUND: &str = "DERODERO/GRAPH/ZERO/ST53A.TIM";

/// The two solo stages, picked by `D_800C2FB8` in `func_80018464` (0 → ST22, else ST23).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    St22,
    St23,
}

impl Stage {
    fn name(self) -> &'static str {
        match self {
            Stage::St22 => "ST22",
            Stage::St23 => "ST23",
        }
    }
}

/// The 320×240 16-bit background, loaded to `0x80138000` and copied straight into the frame
/// buffer by `func_8001A504` every frame.
pub fn background(stage: Stage) -> String {
    format!("DERODERO/GRAPH/SOLO/{}A.TIM", stage.name())
}

#[derive(Clone, Debug)]
pub struct SoloScreen {
    pub window: Frame,
    pub panels: Vec<Frame>,
    pub queue_patch: Frame,
    /// [`DIGIT_FONTS`]: ten digits each.
    pub digits: [Vec<Frame>; 4],
    /// Advance of a space in the text font.
    pub space: i32,
    pub glyph_h: Frame,
    pub glyph_s: Frame,
    pub glyph_d: Frame,
    pub garbage_frame: Frame,
    pub course_arrows: Vec<Frame>,
    /// `0x80056820` record 1: the right half of the two-player frame.
    pub window_right: Frame,
    /// `func_8001D29C`'s text font: letters A–Y.
    pub letters: Vec<Frame>,
    /// The "k-" escape (`0x800562F6`).
    pub glyph_dash: Frame,
}

impl SoloScreen {
    pub fn from_exe(exe: &[u8]) -> Result<Self, Error> {
        let exe = Exe::parse(exe)?;
        let frame = |addr: u32| -> Result<Frame, Error> {
            Frame::from_bytes(exe.at(addr, Frame::SIZE)?).ok_or(Error::Truncated)
        };
        let window = frame(WINDOW)?;
        let panels = (0..PANEL_COUNT as u32)
            .map(|i| frame(PANELS + i * Frame::SIZE as u32))
            .collect::<Result<Vec<_>, _>>()?;
        let queue_patch = frame(QUEUE_PATCH)?;
        let font = |base: u32| -> Result<Vec<Frame>, Error> {
            (0..10).map(|d| frame(base + d * Frame::SIZE as u32)).collect()
        };
        let digits = [font(DIGIT_FONTS[0])?, font(DIGIT_FONTS[1])?, font(DIGIT_FONTS[2])?, font(DIGIT_FONTS[3])?];
        let space = i32::from(frame(TEXT_FONT)?.w);
        let (glyph_h, glyph_s, glyph_d) = (frame(GLYPH_H)?, frame(GLYPH_S)?, frame(GLYPH_D)?);
        let garbage_frame = frame(GARBAGE_FRAME)?;
        let course_arrows = (0..COURSE_ARROW_FRAMES)
            .map(|i| frame(COURSE_ARROWS + i * Frame::SIZE as u32))
            .collect::<Result<Vec<_>, _>>()?;
        if window.page != 24
            || panels[0].page != 13
            || queue_patch.page != 18
            || garbage_frame.page != 21
            || digits.iter().flatten().chain([&glyph_h, &glyph_s, &glyph_d]).any(|f| f.page != 12)
        {
            return Err(Error::Unsupported("solo screen records"));
        }
        let window_right = frame(WINDOW + Frame::SIZE as u32)?;
        let letters = (0..25).map(|i| frame(TEXT_FONT + i * Frame::SIZE as u32)).collect::<Result<Vec<_>, _>>()?;
        let glyph_dash = frame(GLYPH_DASH)?;
        Ok(SoloScreen {
            window,
            panels,
            queue_patch,
            digits,
            space,
            glyph_h,
            glyph_s,
            glyph_d,
            garbage_frame,
            course_arrows,
            window_right,
            letters,
            glyph_dash,
        })
    }
}

/// Queue slot scale (out of 128) at slot y (`func_80023294`).
pub fn queue_scale(y: i32) -> i32 {
    (128 - y).max(QUEUE_MIN_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_zeros_are_suppressed_but_still_take_space() {
        assert_eq!(digit_slots(42, 4, false), [None, None, Some(4), Some(2)]);
        assert_eq!(digit_slots(0, 4, false), [None, None, None, Some(0)]);
        assert_eq!(digit_slots(7, 2, true), [Some(0), Some(7)]);
        // Only the low digits fit.
        assert_eq!(digit_slots(12345, 2, false), [Some(4), Some(5)]);
        // Zeros after the first non-zero digit are drawn.
        assert_eq!(digit_slots(1005, 4, false), [Some(1), Some(0), Some(0), Some(5)]);
    }

    #[test]
    fn trial_clock_splits_frames_into_two_digits() {
        // 1 min 2 s 59 frames: 59 / 6 = 9, 59 % 6 = 5 → 10, capped at 9.
        assert_eq!(trial_clock_digits(0x01_02_3B), (1, 2, 9, 9));
        assert_eq!(trial_clock_digits(0x00_00_0D), (0, 0, 2, 2));
    }

    #[test]
    fn counter_switches_to_small_digits_at_ten_thousand() {
        assert_eq!(counter_style(9_999, false), Some((3, 4, 0xD8, 0x28, 0, false)));
        assert_eq!(counter_style(10_000, true), Some((1, 6, 0xE0, 0x78, 1, true)));
        assert_eq!(counter_style(1_000_000, false), Some((1, 8, 0xD8, 0x30, 1, true)));
        assert_eq!(counter_style(100_000_000, false), None);
    }
}
