//! Time Trial (mode 11, `func_80016BB4`): clear a preset course while every chain you make
//! drops garbage onto a second field that nobody plays. The clock stops when that field holds
//! 0x48 settled blobs (12 full rows); a time below the course's best is a new record.
//!
//! Sub-states (`D_800C34AA`, `jtbl_80010180`): 12 the in-game course select
//! (`func_8002C16C`, after "Continue"), 0 init (`func_80017044` and the rest of the handler's
//! first case), 1 play, 3 pause (`func_8002DE14`), 4 the end (`func_8002D628`, the same
//! result/countdown sequence as solo, see [`crate::over`]).
//!
//! The front end's course select (mode 12) lives in [`crate::front`]; it hands over a course
//! number. The layouts come from `FIL2_DAT.OBJ` set 1 (`stackers_formats::presets`).

use crate::bgm::{Director, Inputs, Kind, Request};
use crate::game::{pad, ps, TARGET_SETTLED};
use crate::over::{GameOver, Outcome};
use crate::mascot::{Cues, Mascot};
use crate::pause::Pause;
use crate::Versus;

/// Courses on the select screens (`D_800699D4`, `D_800C30D0[5]`).
pub const COURSES: usize = 5;
/// Course the select screens start on (`D_800B1400 = 2`).
pub const FIRST_CURSOR: i8 = 2;

/// `D_80069694` after the end check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// Player 0 topped out (1).
    Lost,
    /// Player 1 filled up, but not faster than the best (1).
    Slower,
    /// A new best time (2).
    Record,
}

/// SD_CHR scripts of the in-game course select.
pub mod script {
    /// The course list comes in (`func_8002C16C` state 0).
    pub const SELECT_IN: u8 = 0x42;
    /// It leaves after a choice.
    pub const SELECT_OUT: u8 = 0x43;
}

/// The in-game course select (`func_8002C16C`; state `D_800699C0`, timer `D_800B19A4`, the
/// menu's own state `D_800B1340`, arrow frame `D_800B1338`, arrow (`D_800B1344`, `D_800B1348`)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CourseSelect {
    pub state: u8,
    pub timer: u32,
    menu: u8,
    pub cursor: i8,
    pub arrow_frame: usize,
    pub arrow: (i32, i32),
    /// `D_80103658[0..10]`: the list's item palettes (slot i = course i; 5–9 are 6).
    pub palettes: [i16; 10],
    /// The SD VM runs this frame (`func_8002EAD4`; not in state 0).
    pub run_sd: bool,
}

/// `D_80100368`: arrow y for each course (the same list positions as the mode menu).
pub const ARROW_Y: [i32; 5] = [0x43, 0x63, 0x83, 0xA3, 0xC3];
/// `D_800B1344` on entry.
pub const ARROW_X: i32 = 0x28;
/// Frames of `D_80056A88` the arrow plays after a choice.
const ARROW_FRAMES: usize = 0x1B;

impl Default for CourseSelect {
    fn default() -> Self {
        CourseSelect {
            state: 0,
            timer: 0,
            menu: 0,
            cursor: FIRST_CURSOR,
            arrow_frame: 0,
            arrow: (ARROW_X, ARROW_Y[FIRST_CURSOR as usize]),
            palettes: [0, 0, 0, 0, 0, 6, 6, 6, 6, 6],
            run_sd: false,
        }
    }
}

/// What a frame of the select produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectEvent {
    None,
    /// △ while it leaves: start over (sub-state 12, state 0).
    Restart,
    /// The list has gone: play `course` (sub-state 0).
    Play(usize),
}

impl CourseSelect {
    fn step(&mut self, pressed: u16, scripts: &mut Vec<u8>, sounds: &mut Vec<u8>) -> SelectEvent {
        self.timer += 1;
        self.run_sd = true;
        match self.state {
            0 => {
                *self = CourseSelect { state: 1, ..CourseSelect::default() };
                scripts.push(script::SELECT_IN);
                // Returns before the VM.
                self.run_sd = false;
            }
            1 => {
                if self.timer == 0x14 {
                    self.state = 2;
                    self.timer = 0;
                }
            }
            2 => {
                if self.menu == 0 {
                    if pressed == pad::UP {
                        self.cursor -= 1;
                        sounds.push(0xA7);
                    }
                    if pressed == pad::DOWN {
                        self.cursor += 1;
                        sounds.push(0xA7);
                    }
                    if self.cursor >= COURSES as i8 {
                        self.cursor = 0;
                    }
                    if self.cursor < 0 {
                        self.cursor = COURSES as i8 - 1;
                    }
                    if pressed == pad::START || pressed == pad::CROSS {
                        self.menu = 1;
                    }
                }
                if self.menu == 1 {
                    self.menu = 2;
                    sounds.push(0xA8);
                    self.palettes[self.cursor as usize] = 1;
                }
                if self.menu == 2 {
                    self.arrow_frame += 1;
                    if self.arrow_frame >= ARROW_FRAMES {
                        self.arrow_frame = 0;
                        self.menu = 3;
                    }
                }
                if self.menu == 3 {
                    self.state = 3;
                    self.timer = 0;
                    scripts.push(script::SELECT_OUT);
                }
                self.arrow.1 = ARROW_Y[self.cursor as usize];
            }
            _ => {
                let mut ev = SelectEvent::None;
                if pressed == pad::TRIANGLE {
                    ev = SelectEvent::Restart;
                }
                if self.timer >= 0x14 {
                    self.arrow.0 -= 16;
                    self.arrow.1 -= 1;
                }
                if self.timer >= 0x50 && ev == SelectEvent::None {
                    ev = SelectEvent::Play(self.cursor as usize);
                }
                if ev == SelectEvent::Restart {
                    self.state = 0;
                    self.timer = 0;
                }
                return ev;
            }
        }
        SelectEvent::None
    }
}

/// `D_800C34AA`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sub {
    Select,
    Play,
    Paused,
    Over,
}

#[derive(Clone, Debug)]
pub struct TimeTrial {
    pub versus: Versus,
    pub sub: Sub,
    /// `D_800699D4`.
    pub course: usize,
    /// `D_800C30D0`: best time per course (packed like [`crate::Hud::time`]; 0 = none).
    pub best: [u32; COURSES],
    /// `D_80069694`.
    pub finish: Option<Finish>,
    pub over: GameOver,
    pub pause: Pause,
    pub select: CourseSelect,
    /// Set when the run leaves the mode: Quit in the pause menu or the countdown ran out.
    pub outcome: Option<Outcome>,
    /// Sound effects and SD_CHR script starts of the last step.
    pub sounds: Vec<u8>,
    pub scripts: Vec<u8>,
    /// Set by the caller before a step: which ports' connection state (`D_800B1440` /
    /// `D_800B1462`) differs from last frame's (`D_8006626C/D`, saved at the end of every
    /// frame). While the clock runs, a change opens the pause menu like Start. Taken by the step.
    pub port_change: [bool; 2],
    layouts: Vec<Vec<i16>>,
    prev: u16,
    pub frame: u64,
    /// The mascot (`func_800312AC`, actor 0).
    pub mascot: Mascot,
    /// The music director (`func_80039EE4`, mode 11).
    pub bgm: Director,
    /// This frame's music request.
    pub music: Option<Request>,
}

impl TimeTrial {
    /// Start `course` (0–4) with the five course layouts of `FIL2_DAT` set 1.
    pub fn new(seed: u32, layouts: Vec<Vec<i16>>, course: usize, best: [u32; COURSES]) -> TimeTrial {
        let mut t = TimeTrial {
            versus: Versus::time_trial(seed, &[]),
            sub: Sub::Play,
            course: 0,
            best,
            finish: None,
            over: GameOver::default(),
            pause: Pause::default(),
            select: CourseSelect::default(),
            outcome: None,
            sounds: Vec::new(),
            scripts: Vec::new(),
            port_change: [false; 2],
            layouts,
            prev: 0,
            frame: 0,
            mascot: Mascot::time_trial(false),
            bgm: Director::new(Kind::TimeTrial),
            music: None,
        };
        t.start(seed, course);
        t
    }

    /// Sub-state 0.
    fn start(&mut self, seed: u32, course: usize) {
        self.course = course.min(COURSES - 1);
        let layout = self.layouts.get(self.course).map_or(&[][..], Vec::as_slice);
        self.versus = Versus::time_trial(seed, layout);
        self.bgm = Director::new(Kind::TimeTrial);
        self.sub = Sub::Play;
        self.finish = None;
        self.over = GameOver::default();
        self.pause = Pause::default();
        // func_800312AC's first call (D_8010AE00 = 0): func_8003151C. `starts` keeps counting
        // so a renderer sees the restart.
        let starts = self.mascot.starts;
        self.mascot = Mascot::time_trial(self.mascot.latch());
        self.mascot.starts = starts + 1;
    }

    /// `func_800312AC` after sub-state 0: the mascot's mood and lines.
    fn step_mascot(&mut self, held: u16) {
        let g = &self.versus.games[0];
        let cues = Cues {
            chain: g.stats.chain,
            held,
            clock: g.clock_running(),
            paused: self.sub == Sub::Paused,
            over: self.sub == Sub::Over,
            ..Cues::default()
        };
        if let Some(id) = self.mascot.step(cues, || 0) {
            if !self.sounds.contains(&id) {
                self.sounds.push(id);
            }
        }
    }

    /// `D_80100648`: the clock (player 0's [`crate::Hud::time`]).
    pub fn clock(&self) -> u32 {
        self.versus.games[0].hud.time
    }

    /// One 60 Hz frame with pad 1's held buttons (player 1 has no controls).
    pub fn step(&mut self, held: u16) {
        let pressed = held & !self.prev;
        let ports = std::mem::take(&mut self.port_change);
        self.prev = held;
        self.sounds.clear();
        self.scripts.clear();
        self.frame += 1;
        if self.outcome.is_some() {
            self.music = None;
            return;
        }
        // func_80014634 runs the director before the mode handler.
        let ended = match self.finish {
            None => 0,
            Some(Finish::Lost | Finish::Slower) => 1,
            Some(Finish::Record) => 2,
        };
        self.music = self.bgm.step(&Inputs::versus(&self.versus, self.sub == Sub::Play, ended, 0));
        match self.sub {
            Sub::Select => {
                let (mut scripts, mut sounds) = (Vec::new(), Vec::new());
                let ev = self.select.step(pressed, &mut scripts, &mut sounds);
                self.scripts = scripts;
                self.sounds = sounds;
                if let SelectEvent::Play(course) = ev {
                    let seed = self.versus.seed();
                    self.start(seed, course);
                }
            }
            Sub::Play => self.play(held, pressed, ports),
            Sub::Paused => {
                self.versus.hold([held, 0]);
                let mut sounds = Vec::new();
                self.pause.step(pressed, |id| sounds.push(id));
                self.sounds = sounds;
                if self.pause.music_stop {
                    self.music = Some(Request::Stop);
                }
                self.step_spans();
                match self.pause.outcome {
                    Some(Outcome::Retry) => {
                        let (seed, course) = (self.versus.seed(), self.course);
                        self.start(seed, course);
                    }
                    Some(Outcome::Title) => self.outcome = Some(Outcome::Title),
                    None if !self.pause.active => self.sub = Sub::Play,
                    None => {}
                }
            }
            Sub::Over => {
                let mut sounds = Vec::new();
                let moving = self.over.step(pressed, |id| sounds.push(id));
                self.sounds = sounds;
                self.scripts = self.over.scripts.clone();
                if let Some(n) = self.over.xa {
                    self.music = Some(Request::Xa(n));
                }
                if moving {
                    self.step_end_fields();
                }
                self.step_spans();
                // After func_8002D628.
                self.step_mascot(held);
                match self.over.outcome {
                    // func_8002D628 phase 4 in mode 11: the in-game course select.
                    Some(Outcome::Retry) => {
                        self.sub = Sub::Select;
                        self.select = CourseSelect::default();
                    }
                    Some(Outcome::Title) => self.outcome = Some(Outcome::Title),
                    None => {}
                }
            }
        }
    }

    /// `func_800258A8` runs with the field draw in every sub-state that draws the fields.
    fn step_spans(&mut self) {
        for g in &mut self.versus.games {
            g.field.step_span_segments();
        }
    }

    /// Sub-state 1: the two-player frame, the pause check (`D_800667A4` set, Start alone or a
    /// port's connection changing), then `func_8002DA70`.
    fn play(&mut self, held: u16, pressed: u16, ports: [bool; 2]) {
        self.versus.step([held, 0]);
        let mut sounds = Vec::new();
        for g in &self.versus.games {
            for &id in &g.sounds {
                if !sounds.contains(&id) {
                    sounds.push(id);
                }
            }
        }
        self.sounds = sounds;
        // After func_8002C770, before the pause check and func_8002DA70.
        self.step_mascot(held);
        if self.versus.games[0].clock_running() && (pressed == pad::START || ports.contains(&true)) {
            self.pause.begin();
            self.sub = Sub::Paused;
        }
        self.end_check();
    }

    /// `func_8002DA70` (with `func_8002DCD8`): player 0 in state 10 loses; otherwise player 1
    /// holding 0x48 settled blobs finishes, and a time under the best (or the first time on the
    /// course) is the new best.
    fn end_check(&mut self) {
        let finish = if self.versus.games[0].player.state == ps::GAME_OVER {
            Finish::Lost
        } else if self.versus.games[1].settled() >= TARGET_SETTLED {
            let best = &mut self.best[self.course];
            let clock = self.versus.games[0].hud.time;
            if *best == 0 || clock < *best {
                *best = clock;
                Finish::Record
            } else {
                Finish::Slower
            }
        } else {
            return;
        };
        self.finish = Some(finish);
        for g in &mut self.versus.games {
            g.stop_clock();
        }
        self.sub = Sub::Over;
        self.pause.active = false;
        self.over.begin(finish == Finish::Record, false);
        // func_8003B2B8, mode 11.
        let v = &mut self.versus;
        if finish == Finish::Record {
            v.games[1].field.pop_special();
            self.sounds.push(0x1E);
        } else {
            for i in 0..2 {
                let (games, rng) = (&mut v.games, &mut v.rng);
                games[i].field.burst(|| rng.next());
            }
        }
        v.games[0].player.state = ps::GAME_OVER;
    }

    /// `func_8003B03C`, mode 11.
    fn step_end_fields(&mut self) {
        if self.finish == Some(Finish::Record) {
            let popped = self.versus.games[1].field.pop_special_step();
            // func_8003B644 always takes them off player 1's grey count.
            let f = &mut self.versus.games[1].field;
            f.grey = f.grey.wrapping_sub(popped);
        } else {
            for g in &mut self.versus.games {
                g.field.burst_step();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::st;

    fn layouts() -> Vec<Vec<i16>> {
        // A few blobs per course; course n puts colour n+1 in the bottom row's first cell.
        (0..COURSES).map(|n| {
            let mut l = vec![0x80i16; 12 * 6];
            l[11 * 6] = n as i16 + 1;
            l
        }).collect()
    }

    #[test]
    fn the_course_falls_into_player_0() {
        let mut t = TimeTrial::new(5, layouts(), 3, [0; COURSES]);
        assert_eq!(t.course, 3);
        assert_eq!(t.versus.games[0].field.cell(1, 17).color, 4);
        assert!(t.versus.games[1].field.cell(1, 17).is_empty());
        for _ in 0..10 {
            t.step(0);
        }
        assert_eq!(t.versus.games[0].field.cell(1, 17).state, st::SETTLED);
    }

    #[test]
    fn the_clock_starts_after_the_long_intro_and_caps() {
        let mut t = TimeTrial::new(5, layouts(), 0, [0; COURSES]);
        for _ in 0..0xB4 {
            t.step(0);
        }
        assert_eq!(t.clock(), 0);
        t.step(0);
        t.step(0);
        assert!(t.clock() > 0);
        assert_eq!(crate::game::tick_clock(0x3B_3B3B, true), 0x3B_3B3B);
        assert_eq!(crate::game::tick_clock(0x3B_3B3B, false), 0x100_0000);
    }

    #[test]
    fn player_1_takes_garbage_and_never_spawns() {
        let mut t = TimeTrial::new(5, layouts(), 0, [0; COURSES]);
        t.versus.games[1].garbage.incoming[0] = 18;
        for _ in 0..400 {
            t.step(0);
            assert!(!t.versus.games[1].piece_active());
        }
        let f = &t.versus.games[1].field;
        let special = (1..7)
            .flat_map(|c| (0..18).map(move |r| (c, r)))
            .filter(|&(c, r)| f.cell(c, r).color == 6 && f.cell(c, r).state == st::SETTLED)
            .count();
        assert_eq!(special, 18);
    }

    #[test]
    fn filling_player_1_is_a_record_then_slower() {
        let mut t = TimeTrial::new(5, layouts(), 1, [0; COURSES]);
        for _ in 0..200 {
            t.step(0);
        }
        // Fill twelve rows of player 1 directly.
        for c in 1..7 {
            for r in 6..18 {
                t.versus.games[1].field.put(c, r, 6);
            }
        }
        for _ in 0..3 {
            t.step(0);
        }
        assert_eq!(t.finish, Some(Finish::Record));
        assert_eq!(t.sub, Sub::Over);
        let best = t.best[1];
        assert!(best > 0);
        assert_eq!(t.over.message, crate::over::script::NEW_RECORD);
        // The pop empties player 1's field.
        for _ in 0..60 {
            t.step(0);
        }
        let f = &t.versus.games[1].field;
        assert!((1..7).all(|c| (0..18).all(|r| f.cell(c, r).state == st::EMPTY)));

        let mut t = TimeTrial::new(6, layouts(), 1, t.best);
        for _ in 0..(0xB4 + 600) {
            t.step(0);
        }
        for c in 1..7 {
            for r in 6..18 {
                t.versus.games[1].field.put(c, r, 6);
            }
        }
        t.step(0);
        t.step(0);
        assert_eq!(t.finish, Some(Finish::Slower));
        assert_eq!(t.best[1], best);
    }

    #[test]
    fn continue_opens_the_course_select() {
        let mut t = TimeTrial::new(5, layouts(), 0, [0; COURSES]);
        t.versus.games[0].player.state = ps::GAME_OVER;
        t.step(0);
        assert_eq!(t.finish, Some(Finish::Lost));
        t.step(0);
        for _ in 0..301 {
            t.step(0);
        }
        // Countdown: Start continues.
        t.step(pad::START);
        for _ in 0..62 {
            if t.sub == Sub::Select {
                break;
            }
            t.step(0);
        }
        assert_eq!(t.sub, Sub::Select);
        t.step(0);
        assert_eq!(t.scripts, vec![script::SELECT_IN]);
        for _ in 0..0x14 {
            t.step(0);
        }
        assert_eq!(t.select.state, 2);
        t.step(pad::DOWN);
        t.step(0);
        t.step(pad::CROSS);
        assert_eq!(t.select.cursor, 3);
        for _ in 0..(0x1B + 0x50 + 2) {
            t.step(0);
        }
        assert_eq!(t.sub, Sub::Play);
        assert_eq!(t.course, 3);
        assert_eq!(t.versus.games[0].field.cell(1, 17).color, 4);
    }

    /// The five courses of the user's own `FIL2_DAT.OBJ` (set 1), from the extracted disc.
    fn disc_layouts() -> Vec<Vec<i16>> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../extracted/disc/DERODERO/DATA/FIL2_DAT.OBJ");
        let d = std::fs::read(path).expect("extracted FIL2_DAT.OBJ");
        let u32_at = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as usize - 0x8017_0000;
        let set = u32_at(4);
        (0..COURSES)
            .map(|n| {
                let l = u32_at(set + 4 * n);
                (0..12 * 6).map(|k| i16::from_le_bytes([d[l + 2 * k], d[l + 2 * k + 1]])).collect()
            })
            .collect()
    }

    /// `FRAME_DUMP=/tmp/t.txt cargo test -p stackers-core trial::tests::frame_dump -- --ignored`
    /// then `python3 tools/framediff.py trial /tmp/t.txt`. Random held pads on pad 1 (no Start),
    /// on a random course, until the end sequence has run 600 frames.
    #[test]
    #[ignore]
    fn frame_dump() {
        use crate::rng::BiosRand;
        use crate::set_match::tests::versus_line;
        use std::fmt::Write as _;
        let path = std::env::var("FRAME_DUMP").expect("FRAME_DUMP");
        let games: u32 = std::env::var("FRAME_GAMES").map_or(10, |v| v.parse().unwrap());
        let mut rng = BiosRand::new(std::env::var("FRAME_SEED").map_or(333, |v| v.parse().unwrap()));
        let layouts = disc_layouts();
        let mut out = String::new();
        let buttons = [pad::LEFT, pad::RIGHT, pad::DOWN, pad::CROSS, pad::CIRCLE];
        for _ in 0..games {
            let seed = rng.next() * 7919 + rng.next();
            let course = (rng.next() % COURSES as u32) as usize;
            let new = || TimeTrial::new(seed, layouts.clone(), course, [0; COURSES]);
            let mut t = new();
            let mut pads = Vec::new();
            let mut held = 0u16;
            let mut after = 0;
            // Every other game, pad 1 is played by the CPU AI (its own rand, so the replay is
            // only pads) to reach the record path: player 2's field filled with garbage.
            let mut ai = (rng.next() & 1 == 0).then(|| {
                (crate::cpu::Cpu::new(crate::cpu::Params::stage(1, 13), 0), BiosRand::new(rng.next()))
            });
            while t.sub != Sub::Select && t.outcome.is_none() && pads.len() < 30_000 && after < 600 {
                if t.sub == Sub::Over {
                    after += 1;
                }
                if let (Some((cpu, r)), Sub::Play) = (&mut ai, t.sub) {
                    held = cpu.step(&t.versus.games[0], r);
                } else if rng.next() % 6 == 0 {
                    held = 0;
                    for &b in &buttons {
                        if rng.next() % 4 == 0 {
                            held |= b;
                        }
                    }
                }
                pads.push(held);
                t.step(held);
            }
            eprintln!("course {course} ai {} {:?} {} frames", ai.is_some(), t.finish, pads.len());
            write!(out, "T {seed} {course} ").unwrap();
            for h in &pads {
                write!(out, "{h},").unwrap();
            }
            out.push('\n');
            let mut t = new();
            writeln!(out, "F {}", versus_line(&t.versus)).unwrap();
            for &h in &pads {
                t.step(h);
                writeln!(out, "F {}", versus_line(&t.versus)).unwrap();
            }
        }
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn a_port_change_pauses_once_the_clock_runs() {
        let mut t = TimeTrial::new(5, layouts(), 0, [0; COURSES]);
        t.port_change = [true, false];
        t.step(0);
        assert_eq!(t.sub, Sub::Play);
        while !t.versus.games[0].clock_running() {
            t.step(0);
        }
        t.port_change = [false, true];
        t.step(0);
        assert_eq!((t.sub, t.pause.active), (Sub::Paused, true));
    }
}
