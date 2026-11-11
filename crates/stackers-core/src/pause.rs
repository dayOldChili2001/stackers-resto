//! The solo pause menu (mode 4 sub-state 3, `func_8002DE14` → `func_8002E0F4`).
//!
//! Start during play (only while the clock runs: after the intro, before game over) enters it at
//! the end of the frame (`func_80017098`, init `func_8002DD80`). While paused the mode handler
//! only draws: neither the player, the cells, the queue nor the clock advance. The menu is
//! Continue / Quit / Retry; Quit asks for confirmation.

use crate::game::pad;
use crate::over::Outcome;

/// Frames from choosing Retry or Quit to leaving (`D_8006850C ≥ 0x3D`).
pub const LEAVE_FRAMES: u32 = 0x3D;

/// Menu sound (`func_80040740(0xA7, 1)`), when an item is chosen.
pub const SE_CHOOSE: u8 = 0xA7;

/// `D_80103248`.
pub mod menu {
    /// Choosing an item.
    pub const SELECT: u8 = 0;
    /// Retry chosen: leave after [`super::LEAVE_FRAMES`].
    pub const RETRY: u8 = 1;
    /// Quit confirmed: leave after [`super::LEAVE_FRAMES`].
    pub const QUIT: u8 = 2;
    /// "Quit?" confirmation: ✕ quits, △ goes back.
    pub const CONFIRM: u8 = 3;
    /// Continue chosen: resume on the next frame.
    pub const CONTINUE: u8 = 4;
}

/// Palette of the chosen item (`D_8010365A..E` = 5).
pub const CLUT_CHOSEN: i16 = 5;
/// Palette of the highlighted item.
pub const CLUT_CURSOR: i16 = 1;
/// Palette of the other items.
pub const CLUT_ITEM: i16 = 0;
/// Brightness of the highlighted item (and everything that isn't an item).
pub const BRIGHT_ON: u8 = 0x80;
/// Brightness of the other items.
pub const BRIGHT_OFF: u8 = 0x3C;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pause {
    pub active: bool,
    /// `D_80103248`, see [`menu`].
    pub menu: u8,
    /// `D_800B1400`: 0 Continue, 1 Quit, 2 Retry.
    pub cursor: u8,
    /// `D_8006850C`, incremented at the top of every menu frame.
    pub timer: u32,
    /// Palettes of Continue, Quit, Retry (`D_8010365A`, `D_8010365C`, `D_8010365E`).
    pub item_clut: [i16; 3],
    /// Brightness of Continue, Quit, Retry (`D_80103611..13`).
    pub item_bright: [u8; 3],
    /// Set when the menu chose to leave the game.
    pub outcome: Option<Outcome>,
    /// 3 (Continue / Quit / Retry) or 2 in modes 13–15 (`func_8002E79C`: Continue / Quit).
    pub items: u8,
    /// Set on the frame Retry or Quit leaves: `func_80040A7C(1, 2)` stops the music, even when
    /// Start on that same frame cancels a Retry.
    pub music_stop: bool,
}

impl Default for Pause {
    fn default() -> Self {
        let mut p = Pause {
            active: false,
            menu: menu::SELECT,
            cursor: 0,
            timer: 0,
            item_clut: [0; 3],
            item_bright: [0; 3],
            outcome: None,
            items: 3,
            music_stop: false,
        };
        p.highlight();
        p
    }
}

impl Pause {
    /// `func_8002DD80`.
    pub(crate) fn begin(&mut self) {
        *self = Pause { active: true, ..Default::default() };
    }

    /// `func_8002E514`, modes 13–15: the menu without Retry.
    pub(crate) fn begin_versus(&mut self) {
        *self = Pause { active: true, items: 2, ..Default::default() };
    }

    /// The per-frame highlight of menu state 0.
    fn highlight(&mut self) {
        for i in 0..usize::from(self.items) {
            let on = usize::from(self.cursor) == i;
            self.item_clut[i] = if on { CLUT_CURSOR } else { CLUT_ITEM };
            self.item_bright[i] = if on { BRIGHT_ON } else { BRIGHT_OFF };
        }
    }

    /// One frame of mode 4 sub-state 3. `pressed` is the edge word `D_800B1908`; the original
    /// compares it for equality, so a button only counts when it is the only new press.
    pub(crate) fn step(&mut self, pressed: u16, mut sound: impl FnMut(u8)) {
        self.timer += 1;
        self.music_stop = false;
        match self.menu {
            menu::SELECT => {
                if pressed == pad::START {
                    self.active = false;
                    return;
                }
                if pressed == pad::UP {
                    self.cursor = if self.cursor == 0 { self.items - 1 } else { self.cursor - 1 };
                }
                if pressed == pad::DOWN {
                    self.cursor = if self.cursor + 1 == self.items { 0 } else { self.cursor + 1 };
                }
                self.highlight();
                if pressed == pad::CROSS {
                    sound(SE_CHOOSE);
                    self.timer = 0;
                    self.item_clut[usize::from(self.cursor)] = CLUT_CHOSEN;
                    self.menu = [menu::CONTINUE, menu::CONFIRM, menu::RETRY][usize::from(self.cursor)];
                }
            }
            menu::RETRY | menu::QUIT => {
                if self.timer >= LEAVE_FRAMES {
                    // func_80040A7C(1, 2) stops the music first.
                    self.music_stop = true;
                    self.outcome =
                        Some(if self.menu == menu::RETRY { Outcome::Retry } else { Outcome::Title });
                }
            }
            menu::CONFIRM => {
                if pressed == pad::CROSS {
                    self.menu = menu::QUIT;
                    self.timer = 0;
                } else if pressed == pad::TRIANGLE {
                    self.menu = menu::SELECT;
                    self.timer = 0;
                }
            }
            menu::CONTINUE => self.active = false,
            _ => {}
        }
        // func_80017098 sub-state 3, after the menu: Start resumes in any menu state. It
        // overwrites the sub-state Retry just set (0), so Start on the leaving frame cancels a
        // Retry (the music stays stopped); Quit's mode 0 stands.
        if pressed == pad::START {
            self.active = false;
            if self.outcome == Some(Outcome::Retry) {
                self.outcome = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opens the menu, picks Retry and waits until the frame before it leaves.
    fn retry_chosen() -> Pause {
        let mut p = Pause::default();
        p.begin();
        p.step(pad::UP, |_| {});
        p.step(0, |_| {});
        p.step(pad::CROSS, |_| {});
        assert_eq!(p.menu, menu::RETRY);
        while p.timer + 1 < LEAVE_FRAMES {
            p.step(0, |_| {});
        }
        p
    }

    #[test]
    fn retry_leaves_after_its_frames_and_stops_the_music() {
        let mut p = retry_chosen();
        p.step(0, |_| {});
        assert_eq!((p.outcome, p.music_stop), (Some(Outcome::Retry), true));
    }

    #[test]
    fn start_on_the_leaving_frame_cancels_a_retry_but_not_the_music_stop() {
        let mut p = retry_chosen();
        p.step(pad::START, |_| {});
        assert_eq!((p.outcome, p.active, p.music_stop), (None, false, true));
    }
}
