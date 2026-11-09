//! Chain Reaction's mascot (mode 7 only, `func_80031730`): the girl in the bath beside the
//! tower, and her voice lines.
//!
//! The picture itself is an SD_CHR "actor" script (drawn by the web crate); this module keeps
//! the state the original decides it from. Her mood (`D_8005F3C0`) picks the script: 0 while
//! chains of 2 or more keep coming, 1 (she dozes off) once 5 minutes of play pass without one.
//! The voice lines (`func_80032CCC`) answer chains, fill silences and comment on the final
//! record. They draw from the piece RNG, so they change which pieces come next.

/// `func_8003189C`: frames without a chain of 2+ before the mood turns to 1.
pub const DOZE_AFTER: u32 = 18_000;
/// `D_8005F648`: an idle line is rolled once this many frames have passed (> 0xE10).
const IDLE_LINE: u32 = 0xE10;
/// `D_8005F650` / `D_8005F64C`: half-hour and hour counters (they only reset, nothing plays).
const HALF_HOUR: u32 = 0x1_A5E0;
const HOUR: u32 = 0x3_4BC0;
/// `D_8005F644`, capped here (frames since the last line; read by nothing in mode 7).
const QUIET_CAP: u32 = 0x258;

/// Voice effects (bank `ASA_DERO`).
pub mod voice {
    pub const IDLE_A: u8 = 0x76;
    pub const CHAIN: u8 = 0x77;
    pub const IDLE_B: u8 = 0x78;
    pub const BIG_CHAIN: u8 = 0x7D;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mascot {
    /// `D_8005F3C0`: 0 awake, 1 dozing.
    pub mood: u8,
    /// Bumped whenever the actor script (re)starts (`func_80031A40`), so a renderer can tell.
    pub starts: u32,
    /// `D_8005F63C`: play frame of the last chain of 2+ (or of the start).
    since: u32,
    /// `D_8005F648`, `D_8005F650`, `D_8005F64C`, `D_8005F644`.
    idle: u32,
    half_hour: u32,
    hour: u32,
    quiet: u32,
    /// `D_8005F660`: the chain value the last line answered.
    answered: u16,
    /// `D_8005F658`: the pause roll was made.
    pause_rolled: bool,
    /// `D_8005F664`: the game-over line was decided.
    farewell: bool,
}

/// What the commentary needs to know about the game this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cues {
    /// `D_800C3068`: clears so far in the current chain.
    pub chain: u16,
    /// `D_80066AD0`.
    pub play_frames: u32,
    /// Mode 7 sub-state 3.
    pub paused: bool,
    /// `D_80069694` ≠ 0.
    pub over: bool,
    /// `D_80067224`: the longest chain.
    pub record: u32,
}

impl Mascot {
    /// `func_80031828` (first call, mode 7 sub-state 0).
    pub fn new(play_frames: u32) -> Self {
        Mascot { since: play_frames, starts: 1, ..Default::default() }
    }

    /// One call of `func_80031730` (every mode 7 frame: play, pause and game over). `rand` is
    /// the BIOS `rand` the pieces use. Returns the voice effect to play, if any.
    pub fn step(&mut self, cues: Cues, mut rand: impl FnMut() -> u32) -> Option<u8> {
        self.idle += 1;
        self.hour += 1;
        self.half_hour += 1;
        // D_800699D0 is 0xFF in mode 7, so the rest always runs.
        if !cues.paused {
            self.update_mood(cues);
        }
        self.commentary(cues, &mut rand)
    }

    /// `func_8003189C`.
    fn update_mood(&mut self, cues: Cues) {
        if cues.chain >= 2 {
            self.since = cues.play_frames;
        }
        let mood = u8::from(self.since + DOZE_AFTER < cues.play_frames);
        if mood != self.mood {
            self.mood = mood;
            self.starts += 1;
        }
    }

    /// `func_80032CCC`.
    fn commentary(&mut self, cues: Cues, rand: &mut impl FnMut() -> u32) -> Option<u8> {
        let mut line = 0;
        if cues.chain != 0 {
            if cues.chain != self.answered {
                self.answered = cues.chain;
                // jtbl_80011F4C on chain − 1; 8 and up take the default.
                line = match cues.chain {
                    1 => 0,
                    2 | 3 | 6 | 7 => voice::CHAIN,
                    _ => voice::BIG_CHAIN,
                };
            }
        } else if self.hour > HOUR {
            if !cues.over {
                self.hour = 0;
                self.answered = 0;
            }
        } else if self.half_hour > HALF_HOUR {
            if !cues.over {
                self.half_hour = 0;
                self.answered = 0;
            }
        } else {
            if self.idle > IDLE_LINE {
                self.idle = 0;
                line = match rand() & 3 {
                    0 => voice::IDLE_A,
                    1 => voice::IDLE_B,
                    _ => 0,
                };
            }
            self.answered = 0;
        }
        if cues.paused {
            if !self.pause_rolled {
                rand();
                self.pause_rolled = true;
            }
        } else {
            self.pause_rolled = false;
        }
        if cues.over {
            line = 0;
            if !self.farewell {
                // As written: only a record of 20–29 gets a line.
                if (20..30).contains(&cues.record) {
                    line = voice::BIG_CHAIN;
                }
                self.farewell = true;
            }
        }
        // D_80109638 (the menu secret) also silences her; presets (D_80100680) skip the
        // effect. Neither is modelled here.
        if cues.paused {
            line = 0;
        }
        if line != 0 {
            self.quiet = 0;
        }
        self.quiet = (self.quiet + 1).min(QUIET_CAP);
        (line != 0).then_some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cues(chain: u16, play_frames: u32) -> Cues {
        Cues { chain, play_frames, ..Default::default() }
    }

    #[test]
    fn chains_get_lines_once_per_value() {
        let mut m = Mascot::new(0);
        let lines: Vec<_> = [0, 1, 1, 2, 2, 3, 4, 5, 6, 7, 8, 9, 0]
            .iter()
            .enumerate()
            .map(|(t, &c)| m.step(cues(c, t as u32), || unreachable!()))
            .collect();
        let (c, b) = (Some(voice::CHAIN), Some(voice::BIG_CHAIN));
        assert_eq!(lines, [None, None, None, c, None, c, b, b, c, c, b, b, None]);
    }

    #[test]
    fn dozes_after_five_quiet_minutes() {
        let mut m = Mascot::new(0);
        let mut rolls = 0;
        for t in 1..=DOZE_AFTER + 1 {
            m.step(cues(0, t), || {
                rolls += 1;
                1
            });
        }
        assert_eq!((m.mood, m.starts), (1, 2));
        // One idle roll per 3601 frames.
        assert_eq!(rolls, (DOZE_AFTER + 1) / (IDLE_LINE + 1));
        m.step(cues(2, DOZE_AFTER + 2), || 0);
        assert_eq!((m.mood, m.starts), (0, 3));
    }

    #[test]
    fn pause_rolls_once_and_mutes() {
        let mut m = Mascot::new(0);
        let mut rolls = 0;
        for t in 0..5 {
            let c = Cues { chain: 2, paused: true, ..cues(0, t) };
            assert_eq!(m.step(c, || { rolls += 1; 0 }), None);
        }
        assert_eq!(rolls, 1);
    }

    #[test]
    fn farewell_only_for_records_in_the_twenties() {
        for (record, line) in [(5, None), (20, Some(voice::BIG_CHAIN)), (29, Some(voice::BIG_CHAIN)), (30, None)] {
            let mut m = Mascot::new(0);
            let c = Cues { over: true, record, ..cues(0, 1) };
            assert_eq!(m.step(c, || 0), line, "record {record}");
            assert_eq!(m.step(c, || 0), None);
        }
    }
}
