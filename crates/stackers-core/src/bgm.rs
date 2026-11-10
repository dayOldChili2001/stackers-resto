//! The in-game music director (`func_80039EE4`, run every frame from `func_80014634`).
//!
//! It starts the mode's CD-DA track once play has run for a while, swaps to the "danger" track
//! (CD-DA index 2) while a field is nearly full and back when it empties, and plays a short XA
//! jingle when the game ends (4 = good, 5 = bad). Everything goes through `func_80040A7C` (XA:
//! kind 1 play, kind 2 stop) and `func_80040AE8` (CD-DA); a stop pauses the drive, whatever it
//! was playing.
//!
//! State (`D_800C34B0`): −1 idle, 0 waiting for play, 1 normal, 2 danger, 3 ended. A change
//! stops the music at once and leaves a request (`D_800C34B2`: 0 the mode's track, 3 danger,
//! 4/5 XA) that `func_8003A050` starts on the 5th frame after (`D_800C34B4` / `D_800C34B6`).

/// What the CD should do this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// `func_80040AE8(index, 1)`: CD-DA table index.
    Cd(u8),
    /// `func_80040A7C(index, 1)`: XA table index.
    Xa(u8),
    /// `func_80040A7C(_, 2)`: pause the drive.
    Stop,
}

impl Request {
    /// The page's encoding: −2 stop, 0–99 a CD-DA index, 100 + n XA entry n.
    pub fn code(self) -> i32 {
        match self {
            Request::Stop => -2,
            Request::Cd(i) => i32::from(i),
            Request::Xa(i) => 100 + i32::from(i),
        }
    }
}

/// The `-1` "nothing this frame" code next to [`Request::code`].
pub fn code(r: Option<Request>) -> i32 {
    r.map_or(-1, Request::code)
}

/// XA jingles.
pub mod xa {
    /// The game ended well: a solo record, a Time Trial record, a two-player win.
    pub const GOOD: u8 = 4;
    /// The game ended badly.
    pub const BAD: u8 = 5;
    /// "Here comes a new challenger" (`func_80016480`).
    pub const CHALLENGER: u8 = 6;
    /// "Game Over" (`func_8002D114` / `func_8002D628`).
    pub const GAME_OVER: u8 = 7;
}

/// CD-DA index of the danger track (disc track 4).
pub const DANGER: u8 = 2;

/// Which helper the mode runs (`jtbl_80011FF4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Mode 4, `func_8003A14C`, track 8.
    Solo,
    /// Mode 11, `func_8003A14C`, track 9.
    TimeTrial,
    /// Mode 7, `func_8003A3C8`, track 10.
    ChainReaction,
    /// Mode 13, `func_8003A4F4`, track by level row.
    VsCpu,
    /// Mode 14, `func_8003A4F4`, track 6.
    SetMatch,
    /// Mode 15, `func_8003A4F4`, track 4.
    Challenger,
}

/// What the director reads each frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    /// The mode's sub-state `D_800C34AA` is 1 (play).
    pub playing: bool,
    /// `D_80066AD0`: frames of play.
    pub frames: u32,
    /// Blobs in each player's field (`D_801005FA` + `D_80100634`, `D_801005FE` + `D_80100636`).
    pub blobs: [u16; 2],
    /// `D_80069694`: 0 still playing, else the loser + 1 (Time Trial: 2 = record).
    pub ended: u8,
    /// `D_800B1238 == 0`: solo game over with a new record.
    pub record: bool,
    /// `D_800662F0`: the level row.
    pub level_row: usize,
    /// `D_800C2FB8`: the menu pad (the human in mode 13).
    pub me: usize,
}

impl Inputs {
    /// A two-field mode's inputs: frames and level row from player 0, both fields' blobs.
    pub fn versus(v: &crate::Versus, playing: bool, ended: u8, me: usize) -> Inputs {
        let [a, b] = &v.games;
        Inputs {
            playing,
            frames: a.play_frames(),
            blobs: [a.blobs(), b.blobs()],
            ended,
            record: false,
            level_row: a.level,
            me,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Director {
    pub kind: Kind,
    /// `D_800C34B0`.
    pub state: i8,
    /// `D_800C34B2`.
    want: u8,
    /// `D_800C34B4`.
    pending: bool,
    /// `D_800C34B6`.
    delay: u8,
}

impl Director {
    /// `func_8001FCA0`: state 0, nothing pending.
    pub fn new(kind: Kind) -> Director {
        Director { kind, state: 0, want: 0, pending: false, delay: 0 }
    }

    /// The mode's track (`D_800B1280`).
    pub fn track(&self, level_row: usize) -> u8 {
        match self.kind {
            Kind::Solo => 8,
            Kind::TimeTrial => 9,
            Kind::ChainReaction => 10,
            Kind::VsCpu => crate::tables::music_for_level(level_row) as u8,
            Kind::SetMatch => 6,
            Kind::Challenger => 4,
        }
    }

    /// One frame of `func_80039EE4`. At most one request comes out: a start is never on the
    /// frame of a stop.
    pub fn step(&mut self, i: &Inputs) -> Option<Request> {
        let mut out = None;
        if self.pending {
            // func_8003A050.
            if self.delay == 4 {
                out = Some(match self.want {
                    0 => Request::Cd(self.track(i.level_row)),
                    3 => Request::Cd(DANGER),
                    n => Request::Xa(n),
                });
                self.delay = 0;
                self.pending = false;
            } else {
                self.delay += 1;
            }
        }
        let stop = match self.kind {
            Kind::Solo | Kind::TimeTrial => self.one_player(i, 0xB5, 0x31, 0x24),
            Kind::ChainReaction => self.one_player(i, 0x90, 0xC1, 0x90),
            Kind::VsCpu | Kind::SetMatch | Kind::Challenger => self.two_player(i),
        };
        if stop {
            out = Some(Request::Stop);
        }
        out
    }

    /// Stop now and start `want` later.
    fn to(&mut self, state: i8, want: u8) -> bool {
        self.state = state;
        self.want = want;
        self.delay = 0;
        self.pending = true;
        true
    }

    /// `func_8003A14C` (solo and Time Trial: `start` 0xB5, `high` 0x31, `low` 0x24, with the end
    /// check) and `func_8003A3C8` (Chain Reaction: 0x90 / 0xC1 / 0x90, no end check).
    fn one_player(&mut self, i: &Inputs, start: u32, high: u16, low: u16) -> bool {
        let chain = self.kind == Kind::ChainReaction;
        let mut stop = false;
        match self.state {
            0 => {
                if i.playing && i.frames >= start {
                    stop = self.to(1, 0);
                }
                return stop;
            }
            1 if i.blobs[0] >= high => stop = self.to(2, 3),
            2 if i.blobs[0] < low => stop = self.to(1, 0),
            1 | 2 => {}
            _ => return false,
        }
        if chain {
            return stop;
        }
        if i.ended & 1 != 0 {
            let good = self.kind == Kind::Solo && i.record;
            stop = self.to(3, if good { xa::GOOD } else { xa::BAD });
        } else if i.ended == 2 {
            stop = self.to(3, xa::GOOD);
        }
        stop
    }

    /// `func_8003A4F4` (modes 13–15): danger when either field is high (level rows below 12
    /// only), back when both are low; at the end XA 5 if the human lost to the CPU, else XA 4.
    fn two_player(&mut self, i: &Inputs) -> bool {
        let mut stop = false;
        match self.state {
            0 => {
                if i.playing && i.frames >= 0x90 {
                    stop = self.to(1, 0);
                }
                return stop;
            }
            1 if (i.blobs[0] >= 0x31 || i.blobs[1] >= 0x31) && i.level_row < 12 => stop = self.to(2, 3),
            2 if i.blobs[0] < 0x24 && i.blobs[1] < 0x24 => stop = self.to(1, 0),
            1 | 2 => {}
            _ => return false,
        }
        let me = i.me as u8 + 1;
        if i.ended == me {
            stop = self.to(3, if self.kind == Kind::VsCpu { xa::BAD } else { xa::GOOD });
        } else if i.ended == (me % 2) + 1 {
            stop = self.to(3, xa::GOOD);
        }
        stop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(d: &mut Director, i: &Inputs, frames: usize) -> Vec<(usize, Request)> {
        (0..frames).filter_map(|f| d.step(i).map(|r| (f, r))).collect()
    }

    #[test]
    fn solo_starts_after_181_frames_then_five_more() {
        let mut d = Director::new(Kind::Solo);
        let mut i = Inputs { playing: true, ..Default::default() };
        let mut out = Vec::new();
        for f in 0..200 {
            i.frames = f;
            if let Some(r) = d.step(&i) {
                out.push((f, r));
            }
        }
        assert_eq!(out, [(0xB5, Request::Stop), (0xB5 + 5, Request::Cd(8))]);
    }

    #[test]
    fn danger_and_back() {
        let mut d = Director::new(Kind::Solo);
        d.state = 1;
        let mut i = Inputs { playing: true, frames: 999, blobs: [0x31, 0], ..Default::default() };
        assert_eq!(run(&mut d, &i, 6), [(0, Request::Stop), (5, Request::Cd(DANGER))]);
        i.blobs[0] = 0x24; // still danger: needs < 36
        assert_eq!(run(&mut d, &i, 6), []);
        i.blobs[0] = 0x23;
        assert_eq!(run(&mut d, &i, 6), [(0, Request::Stop), (5, Request::Cd(8))]);
    }

    #[test]
    fn jingles() {
        let mut d = Director::new(Kind::Solo);
        d.state = 1;
        let i = Inputs { ended: 1, record: true, ..Default::default() };
        assert_eq!(run(&mut d, &i, 6), [(0, Request::Stop), (5, Request::Xa(xa::GOOD))]);
        let mut d = Director::new(Kind::VsCpu);
        d.state = 2;
        let i = Inputs { ended: 1, me: 0, ..Default::default() };
        assert_eq!(run(&mut d, &i, 6), [(0, Request::Stop), (5, Request::Xa(xa::BAD))]);
        let mut d = Director::new(Kind::SetMatch);
        d.state = 1;
        let i = Inputs { ended: 1, me: 0, ..Default::default() };
        assert_eq!(run(&mut d, &i, 6), [(0, Request::Stop), (5, Request::Xa(xa::GOOD))]);
    }

    #[test]
    fn chain_never_ends_and_waits_144() {
        let mut d = Director::new(Kind::ChainReaction);
        let i = Inputs { playing: true, frames: 0x90, ended: 1, ..Default::default() };
        assert_eq!(run(&mut d, &i, 10), [(0, Request::Stop), (5, Request::Cd(10))]);
    }
}
