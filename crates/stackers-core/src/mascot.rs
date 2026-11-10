//! The single-player mascots and their voice lines: Chain Reaction's girl in the bath beside
//! the tower (mode 7, `func_80031730`), solo's (mode 4, `func_80031590`) and Time Trial's
//! (mode 11, `func_800312AC`).
//!
//! The picture itself is an SD_CHR "actor" script (drawn by the web crate); this module keeps
//! the state the original decides it from. The mood (`D_8005F3C0`) picks the script. In mode 7
//! it is 0 while chains of 2 or more keep coming, 1 (she dozes off) once 5 minutes of play pass
//! without one; in solo it follows the current chain (`func_800316C4`). The voice lines
//! (`func_80032CCC` / `func_80032FF0`) answer chains, fill silences and comment on the result.
//! They draw from the piece RNG, so they change which pieces come next.

/// `func_8003189C`: frames without a chain of 2+ before the mood turns to 1.
pub const DOZE_AFTER: u32 = 18_000;
/// `D_8005F648`: an idle line is rolled once this many frames have passed (> 0xE10).
const IDLE_LINE: u32 = 0xE10;
/// `D_8005F650` / `D_8005F64C`: half-hour and hour counters (they only reset, nothing plays).
const HALF_HOUR: u32 = 0x1_A5E0;
const HOUR: u32 = 0x3_4BC0;
/// `D_8005F644`, capped here (frames since the last line; read by nothing in mode 7).
const QUIET_CAP: u32 = 0x258;

/// `func_80032FF0`: solo's idle line is rolled once `D_8005F648` reaches this.
const SOLO_IDLE_LINE: u32 = 0x655;

/// `func_8003132C`: Time Trial's intro pose (mood 7) lasts this many calls.
const TRIAL_INTRO: u32 = 0xF0;

/// Voice effects (bank `ASA_DERO` in mode 7, `NEW_YORK` in solo, `DERO_4` in Time Trial).
pub mod voice {
    pub const IDLE_A: u8 = 0x76;
    pub const CHAIN: u8 = 0x77;
    pub const IDLE_B: u8 = 0x78;
    pub const BIG_CHAIN: u8 = 0x7D;
    /// Solo (`NEW_YORK` programs 60–67).
    pub const SOLO_IDLE_A: u8 = 0x8B;
    pub const SOLO_CHAIN: u8 = 0x8C;
    pub const SOLO_IDLE_B: u8 = 0x8D;
    pub const SOLO_BIG_CHAIN: u8 = 0x92;
    /// Time Trial (`DERO_4` program 2, keys 60–62): ↑, ←, →.
    pub const TRIAL_UP: u8 = 0x73;
    pub const TRIAL_LEFT: u8 = 0x74;
    pub const TRIAL_RIGHT: u8 = 0x75;
}

/// Which mode's mascot this is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// Mode 7 (`func_80031730`), actor table `D_801A8308`.
    #[default]
    ChainReaction,
    /// Mode 4 (`func_80031590`), actor table `D_801A8304`.
    Solo,
    /// Mode 11 (`func_800312AC`), actor table `D_801A8300`.
    TimeTrial,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mascot {
    pub kind: Kind,
    /// `D_8005F3C0`: mode 7 0 awake / 1 dozing; solo 0–3 by chain.
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
    /// `D_8005F640`: Time Trial calls so far, up to [`TRIAL_INTRO`].
    intro: u32,
    /// `D_8005F65C`: a direction's line was said and the pad has not been let go of since.
    /// Never reset by `func_8003151C`, so it carries over to the next course.
    latch: bool,
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
    /// `D_80067224`: the longest chain (mode 7).
    pub record: u32,
    /// `D_8010040C`: the erase number (solo).
    pub erased: u32,
    /// `D_80100240`: pad 1's held buttons (Time Trial).
    pub held: u16,
    /// `D_800667A4`: the clock runs (Time Trial).
    pub clock: bool,
}

impl Mascot {
    /// `func_80031828` (first call, mode 7 sub-state 0).
    pub fn new(play_frames: u32) -> Self {
        Mascot { since: play_frames, starts: 1, ..Default::default() }
    }

    /// `func_80031660` (solo's first call, mode 4 sub-state 0): mood 0, counters reset.
    pub fn solo() -> Self {
        Mascot { kind: Kind::Solo, starts: 1, ..Default::default() }
    }

    /// `func_8003151C` (Time Trial sub-state 0, every course): mood 0, counters reset; the
    /// direction latch is kept.
    pub fn time_trial(latch: bool) -> Self {
        Mascot { kind: Kind::TimeTrial, starts: 1, latch, ..Default::default() }
    }

    /// The direction latch (`D_8005F65C`), to carry into the next course's mascot.
    pub fn latch(&self) -> bool {
        self.latch
    }

    /// One call of `func_80031730` / `func_80031590` / `func_800312AC` (every frame of the
    /// mode after its first: play, pause and game over). `rand` is the BIOS `rand` the pieces
    /// use. Returns the voice effect to play, if any.
    pub fn step(&mut self, cues: Cues, mut rand: impl FnMut() -> u32) -> Option<u8> {
        if self.kind == Kind::TimeTrial {
            // func_800312AC skips func_8003132C in the pause sub-state.
            return if cues.paused { None } else { self.trial_step(cues) };
        }
        self.idle += 1;
        if self.kind == Kind::Solo {
            // func_80031590 counts only D_8005F648; D_800699D0 is 0xFF in mode 4.
            if !cues.paused {
                self.set_mood(solo_mood(cues.chain));
            }
            return self.solo_commentary(cues, &mut rand);
        }
        self.hour += 1;
        self.half_hour += 1;
        // D_800699D0 is 0xFF in mode 7, so the rest always runs.
        if !cues.paused {
            self.update_mood(cues);
        }
        self.commentary(cues, &mut rand)
    }

    /// `func_8003132C`. Its pause roll never runs (it is not called while paused), and
    /// `D_8005F648` only counts up and wraps.
    fn trial_step(&mut self, cues: Cues) -> Option<u8> {
        use crate::pad;
        let mut line = 0;
        let mut say = |latch: &mut bool, id: u8| {
            if !*latch {
                line = id;
                *latch = true;
            }
        };
        let mood = if self.intro < TRIAL_INTRO {
            self.intro += 1;
            7
        } else if !cues.clock {
            0
        } else if cues.chain >= 2 {
            4
        } else if cues.held & pad::RIGHT != 0 {
            say(&mut self.latch, voice::TRIAL_RIGHT);
            3
        } else if cues.held & pad::LEFT != 0 {
            say(&mut self.latch, voice::TRIAL_LEFT);
            2
        } else if cues.held & pad::UP != 0 {
            say(&mut self.latch, voice::TRIAL_UP);
            5
        } else if cues.held & pad::DOWN != 0 {
            6
        } else {
            self.latch = false;
            1
        };
        self.pause_rolled = false;
        if self.idle > IDLE_LINE {
            self.idle = 0;
        }
        self.idle += 1;
        self.set_mood(mood);
        (line != 0 && !cues.over).then_some(line)
    }

    fn set_mood(&mut self, mood: u8) {
        if mood != self.mood {
            self.mood = mood;
            self.starts += 1;
        }
    }

    /// `func_80032FF0`: solo's lines. The hour / half-hour branches of mode 7's version are
    /// there too, but their counters never move in mode 4.
    fn solo_commentary(&mut self, cues: Cues, rand: &mut impl FnMut() -> u32) -> Option<u8> {
        let mut line = 0;
        if cues.chain != 0 {
            if cues.chain != self.answered {
                self.answered = cues.chain;
                // jtbl_80011F6C on chain − 1; 9 and up take the default.
                line = match cues.chain {
                    1 => 0,
                    2 | 3 | 6 | 7 => voice::SOLO_CHAIN,
                    _ => voice::SOLO_BIG_CHAIN,
                };
            }
        } else {
            if self.idle >= SOLO_IDLE_LINE {
                self.idle = 0;
                // jtbl_80011F8C on rand & 0xF; 5–15 say nothing.
                line = match rand() & 0xF {
                    0 | 3 => voice::SOLO_IDLE_A,
                    1 | 2 | 4 => voice::SOLO_IDLE_B,
                    _ => 0,
                };
            }
            self.answered = 0;
        }
        self.pause_roll(cues, rand);
        if cues.over {
            line = 0;
            if !self.farewell {
                // As written: under 1,000 or 5,000–9,999 erased get a line, the rest nothing.
                line = match cues.erased {
                    0..=999 => voice::SOLO_IDLE_A,
                    5000..=9999 => voice::SOLO_BIG_CHAIN,
                    _ => 0,
                };
                self.farewell = true;
            }
        }
        self.finish_line(cues, line)
    }

    /// `D_8005F658`: one `rand()` on the first paused frame.
    fn pause_roll(&mut self, cues: Cues, rand: &mut impl FnMut() -> u32) {
        if cues.paused {
            if !self.pause_rolled {
                rand();
                self.pause_rolled = true;
            }
        } else {
            self.pause_rolled = false;
        }
    }

    /// The common tail: muted while paused (and by the menu secret `D_80109638`, not
    /// modelled; presets skip the effect), then `D_8005F644`.
    fn finish_line(&mut self, cues: Cues, mut line: u8) -> Option<u8> {
        if cues.paused {
            line = 0;
        }
        if line != 0 {
            self.quiet = 0;
        }
        self.quiet = (self.quiet + 1).min(QUIET_CAP);
        (line != 0).then_some(line)
    }

    /// `func_8003189C`.
    fn update_mood(&mut self, cues: Cues) {
        if cues.chain >= 2 {
            self.since = cues.play_frames;
        }
        self.set_mood(u8::from(self.since + DOZE_AFTER < cues.play_frames));
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
        self.pause_roll(cues, rand);
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
        self.finish_line(cues, line)
    }
}

/// `func_800316C4`: solo's mood from the current chain.
fn solo_mood(chain: u16) -> u8 {
    match chain {
        0 | 1 => 0,
        2 | 3 => 1,
        4 | 5 => 2,
        _ => 3,
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
    fn solo_moods_lines_and_idle_rolls() {
        let mut m = Mascot::solo();
        let moods: Vec<_> = [0, 1, 2, 3, 4, 5, 6, 9, 0]
            .iter()
            .map(|&c| {
                m.step(cues(c, 0), || unreachable!());
                m.mood
            })
            .collect();
        assert_eq!(moods, [0, 0, 1, 1, 2, 2, 3, 3, 0]);
        let lines: Vec<_> = [1, 2, 3, 4, 5, 6, 7, 8, 9]
            .iter()
            .map(|&c| m.step(cues(c, 0), || unreachable!()))
            .collect();
        let (c, b) = (Some(voice::SOLO_CHAIN), Some(voice::SOLO_BIG_CHAIN));
        assert_eq!(lines, [None, c, c, b, b, c, c, b, b]);
        // A roll every 0x655 calls without a chain.
        let mut m = Mascot::solo();
        let mut rolls = 0;
        let mut said = vec![];
        for _ in 0..3 * SOLO_IDLE_LINE - 1 {
            said.extend(m.step(cues(0, 0), || {
                rolls += 1;
                rolls - 1
            }));
        }
        assert_eq!((rolls, said.as_slice()), (2, &[voice::SOLO_IDLE_A, voice::SOLO_IDLE_B][..]));
        m.step(cues(0, 0), || 2);
        assert_eq!(m.idle, 0);
        // The game-over line goes by the erase number.
        for (erased, line) in [(999, Some(voice::SOLO_IDLE_A)), (1000, None), (5000, Some(voice::SOLO_BIG_CHAIN)), (10_000, None)] {
            let mut m = Mascot::solo();
            let c = Cues { over: true, erased, ..cues(0, 0) };
            assert_eq!(m.step(c, || 0), line, "erased {erased}");
            assert_eq!(m.step(c, || 0), None);
        }
    }

    #[test]
    fn time_trial_intro_pad_moods_and_latched_lines() {
        use crate::pad;
        let mut m = Mascot::time_trial(false);
        let c = |held: u16, chain: u16| Cues { held, chain, clock: true, ..cues(0, 0) };
        for _ in 0..TRIAL_INTRO {
            assert_eq!(m.step(c(pad::RIGHT, 0), || unreachable!()), None);
            assert_eq!(m.mood, 7);
        }
        let run: Vec<_> = [pad::RIGHT, pad::RIGHT, pad::RIGHT | pad::LEFT, pad::LEFT, 0, pad::LEFT, pad::UP, 0, pad::DOWN, 0, pad::UP]
            .iter()
            .map(|&h| (m.step(c(h, 0), || unreachable!()), m.mood))
            .collect();
        let (r, l, u) = (Some(voice::TRIAL_RIGHT), Some(voice::TRIAL_LEFT), Some(voice::TRIAL_UP));
        assert_eq!(run, [(r, 3), (None, 3), (None, 3), (None, 2), (None, 1), (l, 2), (None, 5), (None, 1), (None, 6), (None, 1), (u, 5)]);
        assert_eq!((m.step(c(pad::UP, 2), || 0), m.mood), (None, 4));
        assert_eq!(m.step(Cues { clock: false, ..c(pad::UP, 0) }, || 0), None);
        assert_eq!(m.mood, 0);
        // Muted after the end; frozen while paused.
        assert_eq!(m.step(Cues { over: true, ..c(pad::UP, 0) }, || 0), None);
        assert_eq!(m.mood, 5);
        let before = m.clone();
        m.step(Cues { paused: true, ..c(pad::LEFT, 0) }, || unreachable!());
        assert_eq!(m, before);
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
