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
/// The two-player pause menu (`func_8002E59C`, modes 13–15), same order and table: Continue,
/// Quit (LEVEL2) and the ✕ legend; "Quit?" (LEVEL2) instead while confirming or leaving; then
/// the "Paused" panel (LEVEL2, monitor included) and the three legends.
pub const VS_PAUSE_ITEMS: [(usize, i32, i32); 3] = [(11, 0x78, 0x60), (12, 0x78, 0x70), (20, 0xE2, 0xD4)];
pub const VS_PAUSE_CONFIRM: (usize, i32, i32) = (13, 0x78, 0x60);
pub const VS_PAUSE_FRAME: [(usize, i32, i32); 4] = [(14, 0x70, 0x50), (17, 0x32, 0xB4), (18, 0x8A, 0xB4), (19, 0xD2, 0xB4)];
/// Palettes `D_80103658` (header), `D_8010365E` (confirmation), set by `func_8002E514`; the
/// legends keep [`PAUSE_LEGEND_CLUT`].
pub const VS_PAUSE_HEADER_CLUT: i16 = 8;
pub const VS_PAUSE_CONFIRM_CLUT: i16 = 5;

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
/// where y is the garbage shake of `func_8001A8B8` (player 1's target field shakes it too), one
/// monitor at (0xC8, 0xA0) with palette 2 (semi-transparent, layer 2),
/// and the garbage field's frame at (0xC0, 0) on layer 2.
pub const TRIAL_MONITOR_AT: (i32, i32) = (0xC8, 0xA0);
pub const TRIAL_MONITOR_CLUT: i16 = 2;
pub const GARBAGE_FRAME_AT: (i32, i32) = (0xC0, 0);
/// Player 2's field: `func_800195FC(2)` adds 0xB8 instead of 8 to x.
pub const P2_FIELD_DX: i32 = 0xB0;

/// `func_8003AD34`: the all-clear sprite's anchor per player, x 8 / 0xB8 + 0x10 and
/// y = 19·16 − camera (−16) − 240, at the field's scale 0x80: over play-area columns 1–6.
pub const ALL_CLEAR_AT: [(i32, i32); 2] = [(0x18, 0x50), (0xC8, 0x50)];
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
pub const VS_PAGES: [i16; 9] = [17, 18, 20, 21, 24, 25, 27, 28, 29];
/// Every page [`vs_page_file`] knows, including the SD images.
pub const VS_FILES: [i16; 12] = [17, 18, 20, 21, 24, 25, 27, 28, 29, 41, 42, 43];

/// The [`vs_page_file`] slot behind SD page 26/27 holding image `n` (1–3, `SdStage::vnao`).
pub fn vs_sd_page(n: u8) -> i16 {
    40 + i16::from(n)
}

pub fn vs_page_file(page: i16) -> Option<&'static str> {
    if let Some(i) = page.checked_sub(VS_CPU_SHEET_SLOT).and_then(|i| usize::try_from(i).ok()) {
        return VS_CPU_SHEET_FILES.get(i).copied();
    }
    if let Some(i) = page.checked_sub(VS_CPU_FILE_SLOT).and_then(|i| usize::try_from(i).ok()) {
        return VS_CPU_FILES.get(i).copied();
    }
    if let Some(i) = page.checked_sub(DEMO_PANEL_SLOT).and_then(|i| usize::try_from(i).ok()) {
        return DEMO_PANEL_FILES.get(i).copied();
    }
    Some(match page {
        // `func_80017DDC`: file 0x52, the difficulty list and its arrow (the re-pick).
        17 => "DERODERO/GRAPH/VS/LEVEL2.TIM",
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
        // vs CPU (`func_80017EB0`): file 0x54 in place of VS_WIN2.
        44 => "DERODERO/GRAPH/VS/1P_WIN.TIM",
        // Mode 15's page 18 (`func_80017B20`): files 0x59 / 0x5A by menu pad
        // ([`vs_challenge_page`]).
        45 => "DERODERO/GRAPH/VS/RANNY1_A.TIM",
        46 => "DERODERO/GRAPH/VS/RANNY2_A.TIM",
        _ => return None,
    })
}

/// [`vs_page_file`] slot of file `0x5B + i` (vs CPU's stage art).
pub const VS_CPU_FILE_SLOT: i16 = 60;
/// Files `0x5B`–`0x7C` of `D_80057298`: the stages' backgrounds (A) and page-18 art (B), then
/// the page-24 opponents.
pub const VS_CPU_FILES: [&str; 34] = [
    "DERODERO/GRAPH/VS/STAGE/ST00A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST00B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST02A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST02B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST04A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST04B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST05A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST05B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST06A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST06B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST08A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST08B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST10A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST10B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST11A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST11B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST12A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST12B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST13A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST13B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST14A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST14B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST25A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST25B.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST26A.TIM",
    "DERODERO/GRAPH/VS/STAGE/ST26B.TIM",
    "DERODERO/GRAPH/VS/LESSON/W_AMIGO.TIM",
    "DERODERO/GRAPH/VS/LESSON/W_ATOS.TIM",
    "DERODERO/GRAPH/VS/LESSON/W_DENKYU.TIM",
    "DERODERO/GRAPH/VS/LESSON/W_ELEPAN.TIM",
    "DERODERO/GRAPH/VS/LESSON/W_NEKO.TIM",
    "DERODERO/GRAPH/VS/LESSON/W_TENSI.TIM",
    "DERODERO/GRAPH/VS/LESSON/W_TAMA.TIM",
    "DERODERO/GRAPH/VS/LESSON/W_QUEN.TIM",
];

/// `func_80017EB0` by stage `D_800662F0`: the background (into `0x80138000`) and the opponent
/// (file 0x75–0x7C, page 24) as file ids; the page-18 art is the background's id + 1.
pub const VS_CPU_STAGE_FILES: [(u8, u8); 14] = [
    (0x73, 0x75),
    (0x63, 0x7A),
    (0x65, 0x7C),
    (0x73, 0x75),
    (0x5D, 0x77),
    (0x6D, 0x78),
    (0x6F, 0x75),
    (0x71, 0x76),
    (0x5F, 0x7C),
    (0x61, 0x7A),
    (0x5B, 0x76),
    (0x67, 0x79),
    (0x69, 0x7B),
    (0x6B, 0x7B),
];

/// [`vs_page_file`] slot of file `0x7D + i`: the vs CPU characters' sheets (RAM slots 1–4).
pub const VS_CPU_SHEET_SLOT: i16 = 100;
/// Files `0x7D`–`0xA9` of `D_80057298`: the characters' SD sheets.
pub const VS_CPU_SHEET_FILES: [&str; 45] = [
    "DERODERO/GRAPH/SD_TIM/VA_TOS1.TIM",
    "DERODERO/GRAPH/SD_TIM/VA_TOS2.TIM",
    "DERODERO/GRAPH/SD_TIM/VA_TOS3.TIM",
    "DERODERO/GRAPH/SD_TIM/VA_TOS11.TIM",
    "DERODERO/GRAPH/SD_TIM/VA_TOS21.TIM",
    "DERODERO/GRAPH/SD_TIM/VA_TOS31.TIM",
    "DERODERO/GRAPH/SD_TIM/VAMIGO1.TIM",
    "DERODERO/GRAPH/SD_TIM/VAMIGO2.TIM",
    "DERODERO/GRAPH/SD_TIM/VAMIGO3.TIM",
    "DERODERO/GRAPH/SD_TIM/VAMIGO11.TIM",
    "DERODERO/GRAPH/SD_TIM/VAMIGO21.TIM",
    "DERODERO/GRAPH/SD_TIM/VAMIGO31.TIM",
    "DERODERO/GRAPH/SD_TIM/VAMIGO13.TIM",
    "DERODERO/GRAPH/SD_TIM/VAMIGO23.TIM",
    "DERODERO/GRAPH/SD_TIM/VAMIGO33.TIM",
    "DERODERO/GRAPH/SD_TIM/VDENKYU1.TIM",
    "DERODERO/GRAPH/SD_TIM/VDENKYU2.TIM",
    "DERODERO/GRAPH/SD_TIM/VDENKYU3.TIM",
    "DERODERO/GRAPH/SD_TIM2/VGAKI1.TIM",
    "DERODERO/GRAPH/SD_TIM2/VGAKI2.TIM",
    "DERODERO/GRAPH/SD_TIM2/VGAKI3.TIM",
    "DERODERO/GRAPH/SD_TIM2/VGAKI11.TIM",
    "DERODERO/GRAPH/SD_TIM2/VGAKI21.TIM",
    "DERODERO/GRAPH/SD_TIM2/VGAKI31.TIM",
    "DERODERO/GRAPH/SD_TIM2/VNEKO1.TIM",
    "DERODERO/GRAPH/SD_TIM2/VNEKO2.TIM",
    "DERODERO/GRAPH/SD_TIM2/VNEKO3.TIM",
    "DERODERO/GRAPH/SD_TIM2/VNEKO12.TIM",
    "DERODERO/GRAPH/SD_TIM2/VNEKO22.TIM",
    "DERODERO/GRAPH/SD_TIM2/VNEKO32.TIM",
    "DERODERO/GRAPH/SD_TIM2/VAKUTEN1.TIM",
    "DERODERO/GRAPH/SD_TIM2/VAKUTEN2.TIM",
    "DERODERO/GRAPH/SD_TIM2/VAKUTEN3.TIM",
    "DERODERO/GRAPH/SD_TIM2/VAKUTEN4.TIM",
    "DERODERO/GRAPH/SD_TIM2/VNEKO11.TIM",
    "DERODERO/GRAPH/SD_TIM2/VNEKO21.TIM",
    "DERODERO/GRAPH/SD_TIM2/VNEKO31.TIM",
    "DERODERO/GRAPH/SD_TIM2/VTAMA1.TIM",
    "DERODERO/GRAPH/SD_TIM2/VTAMA2.TIM",
    "DERODERO/GRAPH/SD_TIM2/VTAMA3.TIM",
    "DERODERO/GRAPH/SD_TIM2/VTAMA4.TIM",
    "DERODERO/GRAPH/SD_TIM2/VKING1.TIM",
    "DERODERO/GRAPH/SD_TIM2/VKING2.TIM",
    "DERODERO/GRAPH/SD_TIM2/VKING3.TIM",
    "DERODERO/GRAPH/SD_TIM2/VKING4.TIM",
];

/// `func_80018ADC` by stage `D_800662F0`: the file ids loaded into RAM `0x80180000` /
/// `0x80189000` / `0x80192000` / `0x8019B000` (0 = not loaded), which the SD and actor scripts
/// upload as pages 26/27 (see `SdStage::vnao`).
pub const VS_CPU_SHEETS: [[u8; 4]; 14] = [
    [0x89, 0x8A, 0x8B, 0],
    [0x8F, 0x90, 0x91, 0],
    [0x95, 0x96, 0x97, 0],
    [0x83, 0x84, 0x85, 0],
    [0x8C, 0x8D, 0x8E, 0],
    [0x92, 0x93, 0x94, 0],
    [0x86, 0x87, 0x88, 0],
    [0x80, 0x81, 0x82, 0],
    [0x98, 0x99, 0x9A, 0],
    [0x9B, 0x9C, 0x9D, 0x9E],
    [0x7D, 0x7E, 0x7F, 0],
    [0x9F, 0xA0, 0xA1, 0],
    [0xA2, 0xA3, 0xA4, 0xA5],
    [0xA6, 0xA7, 0xA8, 0xA9],
];

/// The [`vs_page_file`] slot holding RAM slot `n` (1–4) on vs CPU stage `stage`.
pub fn vs_cpu_sheet(stage: usize, n: u8) -> Option<i16> {
    let id = *VS_CPU_SHEETS[stage.min(13)].get(usize::from(n).checked_sub(1)?)?;
    (id != 0).then(|| VS_CPU_SHEET_SLOT + i16::from(id) - 0x7D)
}

/// The [`vs_page_file`] slot of file `id` (`0x5B`–`0x7C`).
pub fn vs_cpu_slot(id: u8) -> i16 {
    VS_CPU_FILE_SLOT + i16::from(id) - 0x5B
}

/// vs CPU's background for a stage.
pub fn vs_cpu_background(stage: usize) -> &'static str {
    let (bg, _) = VS_CPU_STAGE_FILES[stage.min(13)];
    VS_CPU_FILES[usize::from(bg - 0x5B)]
}

/// vs CPU's page swaps for a stage (`func_80017EB0` loads over mode 14's pages): 18 → the
/// stage's B art, 20 → 1P_WIN, 24 → the opponent.
pub fn vs_cpu_page(stage: usize, page: i16) -> i16 {
    let (bg, w) = VS_CPU_STAGE_FILES[stage.min(13)];
    match page {
        18 => vs_cpu_slot(bg + 1),
        20 => 44,
        24 => vs_cpu_slot(w),
        p => p,
    }
}

/// Mode 13's score (`func_8001DF84`): the menu pad's, 8 digits, font 1, palette 1, zeros.
pub const VS_CPU_SCORE_AT: (i32, i32) = (0x80, 0xD0);
pub const VS_CPU_SCORE_CLUT: i16 = 1;
/// The stage-won results (`func_8003D448` / `D570` / `D754`), at x 0x20 (player 1 won) or 0xD0:
/// the rank (text palette 1) at y 0x81, the clear time "m'ss"cc" (font 1, palette 1) at y 0xA0
/// with digits at 0xA2, the bonus (9 digits, palette 1) at y 0xBA.
pub const VS_CPU_RESULT_X: [i32; 2] = [0x20, 0xD0];
pub const VS_CPU_RANK_Y: i32 = 0x81;
pub const VS_CPU_TIME_Y: i32 = 0xA0;
pub const VS_CPU_TIME_DIGITS_Y: i32 = 0xA2;
pub const VS_CPU_BONUS_Y: i32 = 0xBA;

/// The set match's full-screen background (file 0x55, `0x80138000`, when `D_800B137C` = 0).
pub const VS_BACKGROUND: &str = "DERODERO/GRAPH/VS/ST18A.TIM";
/// Mode 15's (`D_800B137C` = 1) by menu pad `D_800C2FB8`: files 0x56 / 0x57.
pub const VS_CHALLENGE_BACKGROUNDS: [&str; 2] = ["DERODERO/GRAPH/VS/ST27A.TIM", "DERODERO/GRAPH/VS/ST28A.TIM"];

/// Mode 15's page swap: 18 (TAISEN_A) → RANNY1_A / RANNY2_A by menu pad.
pub fn vs_challenge_page(menu_pad: usize, page: i16) -> i16 {
    if page == 18 {
        45 + (menu_pad & 1) as i16
    } else {
        page
    }
}

/// [`vs_page_file`] slot of the attract demo's panel page 14 (pages 14–17 follow).
pub const DEMO_PANEL_SLOT: i16 = 50;
/// `func_80017C40` (title state 5, before mode 2): files 0x12–0x15 as pages 14–17.
pub const DEMO_PANEL_FILES: [&str; 4] = [
    "DERODERO/GRAPH/DEMO/SETUMEI0.TIM",
    "DERODERO/GRAPH/DEMO/SETUMEI1.TIM",
    "DERODERO/GRAPH/DEMO/SETUMEI2.TIM",
    "DERODERO/GRAPH/DEMO/SETUMEI3.TIM",
];
/// The demo's art is vs CPU stage 6's: ST14A (file 0x6F) as the background, ST14B (0x70) as
/// page 18 and W_AMIGO (0x75) as page 24 (`func_80017C40`).
pub const DEMO_STAGE: usize = 6;

/// The attract demo's page swaps over mode 14's: the panels, and stage 6's art.
pub fn demo_page(page: i16) -> i16 {
    match page {
        14..=17 => DEMO_PANEL_SLOT + page - 14,
        p => vs_cpu_page(DEMO_STAGE, p),
    }
}

/// The ending's (mode 16) files of `D_80057298`, by index: see [`ending_page`].
pub fn ending_file_path(id: u8) -> Option<&'static str> {
    Some(match id {
        0x0E => "DERODERO/GRAPH/HIRAPANE.TIM",
        0x18 => "DERODERO/GRAPH/ENDING/VNEKOE2.TIM",
        0x27 => "DERODERO/GRAPH/OPENING/KABEMONO.TIM",
        0x2F..=0x46 => ENDING_PATHS[usize::from(id - 0x2F)],
        _ => return None,
    })
}

const ENDING_PATHS: [&str; 24] = [
    "DERODERO/GRAPH/ENDING/VA_TOSE.TIM",
    "DERODERO/GRAPH/ENDING/VAKUTENE.TIM",
    "DERODERO/GRAPH/ENDING/VAMIGOE.TIM",
    "DERODERO/GRAPH/ENDING/VAMIGOE2.TIM",
    "DERODERO/GRAPH/ENDING/VDENKYUE.TIM",
    "DERODERO/GRAPH/ENDING/VGAKIE.TIM",
    "DERODERO/GRAPH/ENDING/VGAKIE2.TIM",
    "DERODERO/GRAPH/ENDING/VNEKOE.TIM",
    "DERODERO/GRAPH/ENDING/VTAMAE.TIM",
    "DERODERO/GRAPH/ENDING/VKINGE.TIM",
    "DERODERO/GRAPH/ENDING/STAFF1.TIM",
    "DERODERO/GRAPH/ENDING/STAFF2.TIM",
    "DERODERO/GRAPH/ENDING/STAFF3.TIM",
    "DERODERO/GRAPH/ENDING/STAFF4.TIM",
    "DERODERO/GRAPH/ENDING/STAFF5.TIM",
    "DERODERO/GRAPH/ENDING/STAFF6.TIM",
    "DERODERO/GRAPH/ENDING/STAFF7.TIM",
    "DERODERO/GRAPH/ENDING/STAFF8.TIM",
    "DERODERO/GRAPH/ENDING/STAFF9.TIM",
    "DERODERO/GRAPH/ENDING/ENDKABE1.TIM",
    "DERODERO/GRAPH/ENDING/ENDKABE2.TIM",
    "DERODERO/GRAPH/ENDING/ENDKABE3.TIM",
    "DERODERO/GRAPH/ENDING/ENDKABE4.TIM",
    "DERODERO/GRAPH/ENDING/ENDKABE5.TIM",
];

/// Every file id [`ending_file_path`] knows.
pub const ENDING_FILES: [u8; 27] = {
    let mut out = [0u8; 27];
    out[0] = 0x0E;
    out[1] = 0x18;
    out[2] = 0x27;
    let mut i = 0;
    while i < 24 {
        out[3 + i] = 0x2F + i as u8;
        i += 1;
    }
    out
};

/// The file in texture page `page` during the ending: `func_80030CF0` loads KABEMONO (20),
/// HIRAPANE (21) and the course's characters (course 0: VAMIGOE2 0, VGAKIE2 1, VNEKOE2 2;
/// courses 1/2: VAMIGOE 0, VDENKYUE 1, VGAKIE 2, VAKUTENE 3, VA_TOSE 5, VNEKOE 7, VTAMAE 9,
/// VKINGE 10); `func_80030F7C` (`staff`) then STAFF1–9 over 0–8 and ENDKABE1–5 over 21–25.
pub fn ending_page(course: u8, staff: bool, page: i16) -> Option<u8> {
    if staff {
        match page {
            0..=8 => return Some(0x39 + page as u8),
            21..=25 => return Some(0x42 + (page - 21) as u8),
            _ => {}
        }
    }
    match (course, page) {
        (_, 20) => Some(0x27),
        (_, 21) => Some(0x0E),
        (0, 0) => Some(0x32),
        (0, 1) => Some(0x35),
        (0, 2) => Some(0x18),
        (0, _) => None,
        (_, 0) => Some(0x31),
        (_, 1) => Some(0x33),
        (_, 2) => Some(0x34),
        (_, 3) => Some(0x30),
        (_, 5) => Some(0x2F),
        (_, 7) => Some(0x36),
        (_, 9) => Some(0x37),
        (_, 10) => Some(0x38),
        _ => None,
    }
}

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
/// `D_80100690` (built by `func_8001DA4C`): TEAFONT (0x60, 0x70) 16×16, drawn by
/// `func_8001DCD8` after the tallies at (0x88, 0xC0) and (0xA8, 0xC0), palette 0, layer 0.
pub const VS_TALLY_MARK: Frame = Frame { u: 0x60, v: 0x70, w: 0x10, h: 0x10, dx: 0, dy: 0, page: 25, clut: 25, flip: 0 };
pub const VS_TALLY_MARK_AT: [(i32, i32); 2] = [(0x88, 0xC0), (0xA8, 0xC0)];

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
