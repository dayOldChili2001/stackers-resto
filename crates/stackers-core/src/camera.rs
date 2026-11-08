//! Chain Reaction's camera (mode 7 only): zoom `func_8001B034` and scroll `func_8001AA10`,
//! both run by the mode handler after the play update (and while paused).
//!
//! The field is drawn at `centre + (p − centre)·scale/128` with `p = (x + 8, y − camera − 80)`
//! (`func_800195FC`), so a larger [`Camera::y`] shows lower rows: −16 is the top of the tall
//! field, 0x3B0 its bottom.

use crate::game::pad;

/// Camera limits (`func_8001AA10`'s clamp).
pub const TOP: i32 = -16;
pub const BOTTOM: i32 = 0x3B0;
/// Zoom limits (`D_800FFDB8`, out of 128).
pub const MIN_SCALE: i16 = 0x40;
pub const MAX_SCALE: i16 = 0x80;
/// Camera state 3 keeps the lower falling half this far below the camera.
const FOLLOW_OFFSET: i32 = 0xC8;
/// Camera state 5 waits this long before snapping back to the top.
const RETURN_FRAMES: u16 = 0x1E;

/// `D_801095B0` values.
pub mod cs {
    /// Hold at the bottom (not used by mode 7's init).
    pub const BOTTOM: u8 = 0;
    pub const IDLE: u8 = 1;
    /// R1 / R2 / △ took over.
    pub const MANUAL: u8 = 2;
    /// Following the halves of the last locked pair down.
    pub const FOLLOW: u8 = 3;
    pub const STACKED: u8 = 4;
    /// Waiting to return to the top.
    pub const RETURN: u8 = 5;
}

/// Where the two halves of the last locked pair went (`D_800C3000`, written by `func_8001F67C`):
/// their slots and the pivot's colour. Their current pixel y is read from the field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockRecord {
    pub pivot: (usize, usize),
    pub pivot_color: u8,
    pub satellite: (usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Camera {
    /// `D_800667A8`.
    pub y: i32,
    /// `D_800FFDB8`.
    pub scale: i16,
    /// `D_801095B0`.
    pub state: u8,
    /// `D_80069654`.
    idle: u16,
}

impl Default for Camera {
    /// Mode 7's init (`func_80015AAC` sub-state 0).
    fn default() -> Self {
        Camera { y: TOP, scale: MAX_SCALE, state: cs::IDLE, idle: 0 }
    }
}

impl Camera {
    /// `func_8001B034`: held L2 zooms out, L1 in, both reset; 0x40…0x80.
    pub fn zoom(&mut self, held: u16) {
        if held & pad::L2 != 0 {
            self.scale -= 1;
        }
        if held & pad::L1 != 0 {
            self.scale += 1;
        }
        if held & (pad::L1 | pad::L2) == pad::L1 | pad::L2 {
            self.scale = MAX_SCALE;
        }
        self.scale = self.scale.clamp(MIN_SCALE, MAX_SCALE);
    }

    /// `func_8001AA10`. `y_at(slot)` is the pixel y of a field slot (`+0x10`); `paused` is
    /// mode sub-state 3, where △ doesn't page.
    pub fn step(
        &mut self,
        held: u16,
        pressed: u16,
        player_state: u8,
        paused: bool,
        lock: &LockRecord,
        y_at: impl Fn((usize, usize)) -> i32,
    ) {
        let entry = self.y;
        if self.state == cs::IDLE && player_state == 1 {
            self.state = cs::FOLLOW;
        }
        if matches!(self.state, cs::FOLLOW | cs::STACKED) && player_state == 0 {
            self.state = cs::RETURN;
        }
        if self.state == cs::RETURN && self.idle == RETURN_FRAMES {
            self.y = TOP;
            self.state = cs::IDLE;
        }
        let page = pressed & pad::TRIANGLE != 0 && !paused;
        if pressed & (pad::R1 | pad::R2) != 0 || page {
            self.state = cs::MANUAL;
        }
        if held & (pad::R1 | pad::R2) == pad::R1 | pad::R2 {
            self.state = cs::IDLE;
        }
        match self.state {
            cs::BOTTOM => {
                self.idle = 0;
                self.y = BOTTOM;
            }
            cs::MANUAL => {
                self.idle = 0;
                if held & pad::R2 != 0 {
                    self.y += 6;
                }
                if held & pad::R1 != 0 {
                    self.y -= 6;
                }
                if page {
                    // One screen down, wider when zoomed out; from the bottom it wraps to the top.
                    let extra = (i32::from(MAX_SCALE - self.scale) * 240) / 128;
                    self.y += 0xF0 + extra;
                    if self.y == 0x4A0 + extra {
                        self.y = TOP;
                    }
                }
            }
            cs::FOLLOW => {
                self.idle = 0;
                let (y1, y2) = (y_at(lock.pivot), y_at(lock.satellite));
                if lock.pivot_color != 0 && y1 + 16 == y2 {
                    self.state = cs::STACKED;
                } else {
                    self.y = (y1.max(y2) - FOLLOW_OFFSET).max(entry);
                }
            }
            cs::RETURN => self.idle += 1,
            _ => self.idle = 0,
        }
        self.y = self.y.clamp(TOP, BOTTOM);
    }

    /// `func_80019998` (drawing the pair, after [`Camera::step`]): while idle, keep the pair
    /// 0xC8 px below the camera. It follows a falling pair at once but only moves back up
    /// when the pair is at least 0x20 px higher (a flip or a new pair), then clamps.
    /// `pivot_y` is the pivot's drawn field y ([`crate::game::PairDraw::pivot`]).
    pub fn follow_pair(&mut self, pivot_y: i32) {
        if self.state == cs::IDLE {
            let to = pivot_y - FOLLOW_OFFSET;
            let small_rise = to >= TOP && to < self.y && to > self.y - 0x20;
            if !small_rise {
                self.y = to;
            }
        }
        self.y = self.y.clamp(TOP, BOTTOM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_clamps_and_resets() {
        let mut c = Camera::default();
        for _ in 0..100 {
            c.zoom(pad::L2);
        }
        assert_eq!(c.scale, MIN_SCALE);
        c.zoom(pad::L1 | pad::L2);
        assert_eq!(c.scale, MAX_SCALE);
    }

    #[test]
    fn triangle_pages_down_then_wraps() {
        let mut c = Camera::default();
        let lock = LockRecord::default();
        let mut ys = Vec::new();
        for _ in 0..6 {
            c.step(pad::TRIANGLE, pad::TRIANGLE, 0, false, &lock, |_| 0);
            ys.push(c.y);
        }
        assert_eq!(ys, [0xE0, 0x1D0, 0x2C0, BOTTOM, TOP, 0xE0]);
        assert_eq!(c.state, cs::MANUAL);
    }

    #[test]
    fn follows_the_lower_half_and_returns() {
        let mut c = Camera::default();
        let lock = LockRecord { pivot: (3, 5), pivot_color: 0, satellite: (3, 4) };
        c.step(0, 0, 1, false, &lock, |(_, r)| r as i32 * 16 + 400);
        assert_eq!((c.state, c.y), (cs::FOLLOW, 5 * 16 + 400 - 0xC8));
        // Never moves back up while following.
        c.step(0, 0, 1, false, &lock, |_| 0);
        assert_eq!(c.y, 5 * 16 + 400 - 0xC8);
        c.step(0, 0, 0, false, &lock, |_| 0);
        assert_eq!(c.state, cs::RETURN);
        for _ in 1..RETURN_FRAMES {
            c.step(0, 0, 0, false, &lock, |_| 0);
        }
        assert_eq!(c.y, 5 * 16 + 400 - 0xC8);
        c.step(0, 0, 0, false, &lock, |_| 0);
        assert_eq!((c.state, c.y), (cs::IDLE, TOP));
    }
}
