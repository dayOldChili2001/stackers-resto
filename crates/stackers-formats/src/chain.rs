//! Chain Reaction's screen (mode 7): the tower frame, the zoomed field transform, the
//! off-screen arrows, the depth gauge and the chain counters. Sprite records are read from
//! the executable; positions are the constants the draw code passes (see "Chain Reaction" in
//! `docs/gameplay.md`).

use crate::anm::Frame;
use crate::{exe::Exe, Error};

/// Tower art `D_80055B44`: records 0–4 are 128×256 segments (JIYU12/34/55, pages 18–20),
/// record 5 the 128×24 top edge.
const TOWER: u32 = 0x8005_5B44;
/// Next-pair window (WINDOW, page 23), drawn at [`WINDOW_AT`] by `func_8001A2B4`.
const WINDOW: u32 = 0x8005_5BB0;
/// Arrow animations on COMMON (page 13), 6 frames each, advanced every 9 play frames:
/// "pair above the screen" `D_80056D04`, "below" `D_80056D84`, depth marker `D_80056E04`.
const ARROW_UP: u32 = 0x8005_6D04;
const ARROW_DOWN: u32 = 0x8005_6D84;
const MARKER: u32 = 0x8005_6E04;
const ARROW_FRAMES: usize = 6;
/// Big digits of `func_8002C98C` (`D_80056388`, PSFONT 16×32).
const DIGITS: u32 = 0x8005_6388;

pub const TOWER_SEGMENTS: usize = 5;
/// Segment `i` sits at field (−1, −0x18 + 0x100·i) (`func_8001A270`).
pub const TOWER_X: i32 = -1;
pub const TOWER_Y0: i32 = -0x18;
pub const TOWER_STEP: i32 = 0x100;
pub const WINDOW_AT: (i32, i32) = (0x8C, 0x10);
/// Queue x `D_800B0F90` in mode 7 (`func_80015ED4`).
pub const QUEUE_X: i32 = 0x98;
/// `func_8001A560(0, 1, 2, 5, 2, 10, 0)`: palettes and labels "Chain", "Record",
/// "Falling Blocks".
pub const PANEL_CLUTS: [i16; 3] = [0, 1, 2];
pub const PANEL_LABELS: [usize; 3] = [5, 2, 10];
/// Score (`func_8001DF84` in mode 7).
pub const SCORE_AT: (i32, i32) = (0x86, 0xD0);
/// `func_8002C98C`: chain length then record, three single digits each at these x.
pub const COUNTER_X: [i32; 3] = [0xD8, 0xF0, 0x108];
pub const COUNTER_Y: [i32; 2] = [0x28, 0x70];
/// "Falling Blocks" panel: the pivot at this point, unzoomed (`func_80019998`, mode 7 only).
pub const PANEL_PIVOT: (i32, i32) = (0xF0, 0xC0);
/// Shown in the panel while the pair is off screen, at scale 0x40.
pub const PANEL_ARROW: (i32, i32) = (0xF4, 0xD0);
/// Depth marker x; y = 13·(pair y)/72 + 8, scale 0x40.
pub const MARKER_X: i32 = 0x128;
pub const SMALL: i32 = 0x40;
/// Off-screen arrows: y when the pair is above / below the screen.
pub const ARROW_TOP_Y: i32 = 0x10;
pub const ARROW_BOTTOM_Y: i32 = 0xE8;
/// The field's horizontal zoom centre `D_800C30B8` (`func_80015ED4`), px << 12.
const CENTRE_X: i32 = 0x48 << 12;

/// Texture pages mode 7 loads over the solo ones (`func_800179A8`): slot → disc file.
pub fn page_file(page: i16) -> Option<&'static str> {
    Some(match page {
        18 => "DERODERO/GRAPH/DOMINO/JIYU12.TIM",
        19 => "DERODERO/GRAPH/DOMINO/JIYU34.TIM",
        20 => "DERODERO/GRAPH/DOMINO/JIYU55.TIM",
        23 => "DERODERO/GRAPH/DOMINO/WINDOW.TIM",
        27 => "DERODERO/GRAPH/READYGO2.TIM",
        _ => return None,
    })
}

/// Pages [`page_file`] replaces.
pub const PAGES: [i16; 5] = [18, 19, 20, 23, 27];

/// The vertical zoom centre `D_800C30C0` (px << 12) for camera `y`, recomputed by
/// `func_800195FC`: it slides from the top of the screen (16) at the top of the tower to the
/// middle (120) one screen down, stays there, and reaches the bottom (224) at the bottom.
pub fn centre_y(camera: i32) -> i32 {
    let ramp = |c: i32| (104 * c * 4096) / 240;
    if camera < 0xE0 {
        ramp(camera) + 0x16EEE
    } else if camera <= 0x2C0 {
        0x78 << 12
    } else {
        ramp(camera) - 0xB9111
    }
}

/// Screen position of a point that would be at `(x, y)` unzoomed, at zoom `scale` (out of
/// 128) and camera `camera` (`func_800195FC`'s `centre + (p − centre)·scale/128`).
pub fn zoom(x: i32, y: i32, scale: i32, camera: i32) -> (i32, i32) {
    let cy = centre_y(camera);
    let along = |v: i32, c: i32| (((((v << 12) - c) as i64 * scale as i64) >> 7) as i32 + c) >> 12;
    (along(x, CENTRE_X), along(y, cy))
}

/// Unzoomed screen position of field pixel `(x, y)` (`x + 8`, `y − camera − 0x50`).
pub fn field_to_screen(x: i32, y: i32, camera: i32) -> (i32, i32) {
    (x + 8, y - camera - 0x50)
}

/// `func_8001A2B4`: tower segment `i`'s unzoomed screen position.
pub fn tower_segment(i: usize, camera: i32) -> (i32, i32) {
    (TOWER_X + 8, TOWER_Y0 + TOWER_STEP * i as i32 - (camera - 16))
}

/// Field blobs and the pair are drawn while their zoomed y is in −19…239.
pub fn on_screen(y: i32) -> bool {
    (-0x13..0xF0).contains(&y)
}

/// Tower segments are drawn while their zoomed y is in −0x103…0x1F3.
pub fn tower_on_screen(y: i32) -> bool {
    (-0x103..0x1F4).contains(&y)
}

/// Frame of the arrow animations for `play_frames` (`D_80066AD0`).
pub fn arrow_frame(play_frames: u32) -> usize {
    (play_frames / 9) as usize % ARROW_FRAMES
}

/// Depth marker y for the pivot's field y (`P+4`).
pub fn marker_y(pair_y: i32) -> i32 {
    13 * pair_y / 72 + 8
}

#[derive(Clone, Debug)]
pub struct ChainScreen {
    pub tower: Vec<Frame>,
    pub window: Frame,
    pub arrow_up: Vec<Frame>,
    pub arrow_down: Vec<Frame>,
    pub marker: Vec<Frame>,
    pub digits: Vec<Frame>,
}

impl ChainScreen {
    pub fn from_exe(exe: &[u8]) -> Result<Self, Error> {
        let exe = Exe::parse(exe)?;
        let frame = |addr: u32| Frame::from_bytes(exe.at(addr, Frame::SIZE)?).ok_or(Error::Truncated);
        let list = |addr: u32, n: usize| {
            (0..n).map(|i| frame(addr + (Frame::SIZE * i) as u32)).collect::<Result<Vec<_>, _>>()
        };
        let s = ChainScreen {
            tower: list(TOWER, TOWER_SEGMENTS + 1)?,
            window: frame(WINDOW)?,
            arrow_up: list(ARROW_UP, ARROW_FRAMES)?,
            arrow_down: list(ARROW_DOWN, ARROW_FRAMES)?,
            marker: list(MARKER, ARROW_FRAMES)?,
            digits: list(DIGITS, 10)?,
        };
        let pages_ok = s.tower.iter().all(|f| (18..=20).contains(&f.page))
            && s.window.page == 23
            && s.arrow_up.iter().chain(&s.arrow_down).chain(&s.marker).all(|f| f.page == 13)
            && s.digits.iter().all(|f| f.page == 12);
        if !pages_ok {
            return Err(Error::Unsupported("chain reaction screen records"));
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_centre_pins_top_middle_and_bottom() {
        assert_eq!(centre_y(-16), 16 << 12);
        assert_eq!(centre_y(0xE0), 120 << 12);
        assert_eq!(centre_y(0x2C0), 120 << 12);
        assert_eq!(centre_y(0x3B0) >> 12, 224);
        // Unzoomed is the identity.
        assert_eq!(zoom(37, -12, 128, 500), (37, -12));
        // At half zoom the top of the tower stays put at the top of the screen.
        assert_eq!(zoom(0x48, 16, 64, -16), (0x48, 16));
        assert_eq!(zoom(0x48 + 100, 216, 64, -16), (0x48 + 50, 116));
    }
}
