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
    /// Scripts started during the last step, in order.
    pub scripts: Vec<u8>,
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
        self.message = id;
        self.scripts.push(id);
    }

    /// One frame of `func_8002D628`. Returns whether the field burst steps this frame (every
    /// phase but the first runs `func_8003B03C`).
    pub(crate) fn step(&mut self, pressed: u16, mut sound: impl FnMut(u8)) -> bool {
        self.scripts.clear();
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
                // The original adds 0x101 to the request for the timer and again for ✕, so both
                // on one frame skip a digit through the 0x2xx path; here a frame advances once.
                let tick = self.counter % DIGIT_FRAMES == 0;
                if tick || pressed == pad::CROSS {
                    sound(se::TICK);
                    self.counter = 0;
                    self.show(self.message + 1);
                }
                if pressed == pad::START {
                    self.counter = 0;
                    sound(se::CONTINUE);
                    self.phase = phase::CONTINUE;
                }
                if self.scripts.last() == Some(&script::GAME_OVER) {
                    self.xa = Some(crate::bgm::xa::GAME_OVER);
                    self.limit = TITLE_FRAMES;
                    self.counter = 0;
                    self.phase = phase::GAME_OVER;
                }
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
