//! The solo game-over sequence (mode 4 sub-state 4, `func_8002D628`).
//!
//! When the player reaches state 10, `func_8002DBAC` updates the record, bursts the field
//! (`func_8003B3DC`) and enters this sequence. From then on the mode handler only draws: the
//! field's burst cells fly off (`func_8003B140`), and the "SD" sprite scripts of `SD_CHR.OBJ`
//! show the result, a "Continue?" countdown and finally "Game Over". Script requests go through
//! `D_800C3514` (`0x100 | id` starts script `id`, `func_8002EC34`); here they are
//! [`GameOver::scripts`].

use crate::game::pad;

/// Frames from the result emblem to the countdown (`D_800B1A28` = 0x12C).
pub const RESULT_FRAMES: u32 = 300;
/// Frames per countdown digit.
pub const DIGIT_FRAMES: u32 = 120;
/// Frames from Start to the restart.
pub const RETRY_FRAMES: u32 = 60;
/// Frames "Game Over" stays up before the game returns to the title (`D_800B1A28` = 200).
pub const TITLE_FRAMES: u32 = 200;

/// `SD_CHR.OBJ` script ids.
pub mod script {
    /// New record (erase number ≥ the record): emblem rises from below.
    pub const NEW_RECORD: u8 = 0x13;
    /// No record: emblem drops in from above and bounces.
    pub const NO_RECORD: u8 = 0x14;
    /// "Continue?" with the digit 9; each next id shows one less, down to 0 at 0x21.
    pub const COUNTDOWN_9: u8 = 0x18;
    /// "Game Over" zooming in.
    pub const GAME_OVER: u8 = 0x22;
}

/// Sound effects of the sequence (`func_80040740(id, 1)`).
pub mod se {
    /// Countdown appears or ticks.
    pub const TICK: u8 = 0x1C;
    /// Start pressed during the countdown.
    pub const CONTINUE: u8 = 0x1E;
}

/// `D_800B1420`.
pub mod phase {
    pub const RESULT: u8 = 0;
    pub const WAIT: u8 = 1;
    pub const COUNTDOWN: u8 = 3;
    pub const CONTINUE: u8 = 4;
    pub const GAME_OVER: u8 = 5;
}

/// How the sequence ended; the mode handler then restarts or leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Start during the countdown: mode 4 sub-state 0, a new solo game.
    Retry,
    /// The countdown ran out: mode 0 (title).
    Title,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameOver {
    pub active: bool,
    pub phase: u8,
    /// `D_800C3530`, incremented at the top of every frame.
    pub counter: u32,
    /// `D_800B1A28`.
    pub limit: u32,
    /// `D_800B1238 == 0`: the erase number reached the record.
    pub new_record: bool,
    /// Low byte of `D_800C3514`: the last script started.
    pub message: u8,
    /// Scripts started during the last step, in order (`0x1xx` requests, 16-byte records).
    pub scripts: Vec<u8>,
    /// Scripts started as `0x2xx` requests (20-byte records with start positions,
    /// `SdStage::start_placed`): the countdown's tick and ✕ on one frame.
    pub placed: Vec<u8>,
    /// `D_800C3514` as the countdown left it, before the SD VM strips the high byte.
    pub request: u16,
    /// An XA entry started during the last step (`func_80040A7C(7, 1)` with "Game Over").
    pub xa: Option<u8>,
    pub outcome: Option<Outcome>,
}

impl GameOver {
    /// `func_8002DBAC`'s entry into sub-state 4.
    pub(crate) fn begin(&mut self, new_record: bool) {
        *self = GameOver { active: true, limit: RESULT_FRAMES, new_record, ..Default::default() };
    }

    fn show(&mut self, id: u8) {
        self.request = 0x100 | u16::from(id);
        self.consume();
    }

    /// `func_8002EB38` → `func_8002EC34`: start the requested script, strip the high byte.
    fn consume(&mut self) {
        let id = self.request as u8;
        match self.request >> 8 {
            1 => self.scripts.push(id),
            2 => self.placed.push(id),
            _ => {}
        }
        self.request &= 0xFF;
        self.message = id;
    }

    /// One frame of `func_8002D628`. Returns whether the field burst steps this frame (every
    /// phase but the first runs `func_8003B03C`).
    pub(crate) fn step(&mut self, pressed: u16, mut sound: impl FnMut(u8)) -> bool {
        self.scripts.clear();
        self.placed.clear();
        self.xa = None;
        if self.outcome.is_some() {
            return true;
        }
        self.counter += 1;
        match self.phase {
            phase::RESULT => {
                // func_8002D91C, mode 4.
                self.show(if self.new_record { script::NEW_RECORD } else { script::NO_RECORD });
                self.phase = phase::WAIT;
                self.counter = 0;
                return false;
            }
            phase::WAIT => {
                if self.counter >= self.limit {
                    self.counter = 0;
                    sound(se::TICK);
                    self.show(script::COUNTDOWN_9);
                    self.phase = phase::COUNTDOWN;
                }
            }
            phase::COUNTDOWN => {
                // The timer and ✕ each add 0x101 to the request: both on one frame make a
                // 0x2xx request for the digit after next (placed records), and the exact
                // 0x122 check below misses "Game Over" if that skip lands on it: the countdown
                // then runs on through scripts 0x23… and wraps until it next lands on 0x122.
                if self.counter % DIGIT_FRAMES == 0 {
                    sound(se::TICK);
                    self.counter = 0;
                    self.request = self.request.wrapping_add(0x101);
                }
                if pressed == pad::CROSS {
                    sound(se::TICK);
                    self.counter = 0;
                    self.request = self.request.wrapping_add(0x101);
                }
                if pressed == pad::START {
                    self.counter = 0;
                    sound(se::CONTINUE);
                    self.phase = phase::CONTINUE;
                }
                if self.request == 0x100 | u16::from(script::GAME_OVER) {
                    self.xa = Some(crate::bgm::xa::GAME_OVER);
                    self.limit = TITLE_FRAMES;
                    self.counter = 0;
                    self.phase = phase::GAME_OVER;
                }
                self.consume();
            }
            phase::CONTINUE => {
                if self.counter >= RETRY_FRAMES {
                    self.outcome = Some(Outcome::Retry);
                }
            }
            phase::GAME_OVER => {
                if self.counter >= self.limit {
                    self.outcome = Some(Outcome::Title);
                }
            }
            _ => {}
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the sequence to the countdown, then ✕ until `message` shows.
    fn countdown_at(message: u8) -> GameOver {
        let mut o = GameOver::default();
        o.begin(false);
        while o.phase != phase::COUNTDOWN {
            o.step(0, |_| {});
        }
        while o.message != message {
            o.step(pad::CROSS, |_| {});
            o.step(0, |_| {});
        }
        o
    }

    /// Steps with no input until the frame before the next digit.
    fn to_tick(o: &mut GameOver) {
        while o.counter + 1 < DIGIT_FRAMES {
            o.step(0, |_| {});
        }
    }

    #[test]
    fn countdown_reaches_game_over() {
        let mut o = countdown_at(0x21);
        to_tick(&mut o);
        o.step(0, |_| {});
        assert_eq!((o.scripts.as_slice(), o.phase, o.limit), (&[script::GAME_OVER][..], phase::GAME_OVER, TITLE_FRAMES));
    }

    #[test]
    fn tick_and_cross_on_one_frame_skip_a_digit_as_a_placed_request() {
        let mut o = countdown_at(0x1A);
        to_tick(&mut o);
        o.step(pad::CROSS, |_| {});
        assert!(o.scripts.is_empty());
        assert_eq!((o.placed.as_slice(), o.message, o.request), (&[0x1C][..], 0x1C, 0x1C));
    }

    #[test]
    fn a_skip_onto_game_over_misses_it_and_the_countdown_runs_on() {
        let mut o = countdown_at(0x20);
        to_tick(&mut o);
        o.step(pad::CROSS, |_| {});
        assert_eq!((o.placed.as_slice(), o.phase), (&[script::GAME_OVER][..], phase::COUNTDOWN));
        to_tick(&mut o);
        o.step(0, |_| {});
        assert_eq!((o.scripts.as_slice(), o.phase), (&[0x23][..], phase::COUNTDOWN));
        // 0xFF + 0x101 wraps to a 0x200 request, then it counts up to 0x22 again.
        while o.phase == phase::COUNTDOWN {
            o.step(pad::CROSS, |_| {});
            o.step(0, |_| {});
        }
        assert_eq!((o.phase, o.message), (phase::GAME_OVER, script::GAME_OVER));
    }
}
