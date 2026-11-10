//! Arcade's vs CPU (mode 13, `func_80016480`).
//!
//! Sub-states (`D_800C34AA`, `jtbl_80010150`): 0 init, 1 play, 2 stage won (`func_8003CF2C`),
//! 3 pause, 4 lost (`func_8002D114`, the continue countdown), 10 "here comes a challenger",
//! 11 the stage load (`func_80030394` from state 4). The human plays on the menu pad
//! (`D_800C2FB8`), the AI (`func_80034C60`) on the other. A course (`D_800C30C8`) runs stages
//! `D_800662F0` up to `D_800C2FE0`; both players use row `stage` of the course's level table.

use crate::bgm::{Director, Inputs, Kind, Request};
use crate::cpu::{Cpu, Params};
use crate::mascot::{Mascot, VersusCues};
use crate::game::pad;
use crate::over::{GameOver, Outcome};
use crate::pause::Pause;
use crate::roulette::{ColumnTally, Roulette};
use crate::versus::Versus;

/// `SD_CHR.OBJ` script ids (low byte of `D_800C3514`).
pub mod script {
    /// "Ready, Go!" (`0x100`).
    pub const READY: u8 = 0;
    /// The stage-won emblem, `0x240 + menu pad` (a placed start: [`super::VsCpu::placed`]).
    pub const WON: u8 = 0x40;
    /// "Here comes a new challenger" (`0x244`).
    pub const CHALLENGER: u8 = 0x44;
}

/// Sound effects (`func_80040740`).
pub mod se {
    /// Each 1000 points of the stage bonus moving to the score (`func_8003D7A4`).
    pub const TALLY: u8 = 0x1B;
    /// A second player pressed Start (`func_80016480` sub-state 1).
    pub const CHALLENGER: u8 = 0x1D;
    /// The round ends (`func_8003B2B8`).
    pub const ROUND_END: u8 = 0x1E;
}

/// Stage-won timeline (`D_80066ACC`): phase 1 tallies until 0x5B, the results appear at 0xB5,
/// the bonus counts into the score from 0xF1, and the next stage starts by 0x169.
pub const WON_TALLY_UNTIL: u32 = 0x5B;
pub const WON_SHOW_AT: u32 = 0xB5;
pub const WON_COUNT_AT: u32 = 0xF1;
pub const WON_FLUSH_AT: u32 = 0x12D;
pub const WON_NEXT_AT: u32 = 0x169;
/// Sub-state 10 waits this long for the challenger's Start.
pub const CHALLENGER_FRAMES: u32 = 0x12D;
/// `D_800B1A28` on a loss (`func_8002D0C4`): frames before the countdown.
pub const LOST_WAIT: u32 = 300;
/// The second-to-last stage of the 14-stage courses: after a continue the course ends there.
const CONTINUED_LAST: u8 = 13;

/// Rank strings `D_80012054 + 12·i` and the thresholds `D_80057728` they need to exceed.
pub const RANKS: [&str; 6] = ["SPECIAL A", "A", "B", "C", "D", "LUCKY"];
const RANK_ABOVE: [u32; 5] = [120, 100, 80, 60, 0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sub {
    /// Sub-state 11 states 4–5: two blank frames, then the stage art loads (`func_80017EB0`).
    Load,
    Play,
    Paused,
    Won,
    Lost,
    Challenger,
}

/// How the run leaves mode 13.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// Quit from the pause menu, or the continue countdown ran out.
    Title,
    /// The last stage of the course was won: mode 16.
    Ending,
    /// The other pad pressed Start: mode 15 at difficulty 3 vs 3
    /// ([`crate::set_match::SetMatch::challenger`]), then back to this stage with the winner.
    Challenger(Course),
}

/// What mode 15 keeps of the course it interrupted, to resume it with [`VsCpu::resume`]. The
/// original simply leaves these globals alone (`D_800B137C` = 1 marks the return, `D_800C33F8`
/// the stage).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Course {
    /// `D_800C2FB8`: the human's pad; on the way back, the match winner's.
    pub menu_pad: usize,
    /// `D_800C30C8`.
    pub course: u8,
    /// `D_800C33F8`: the stage to return to.
    pub stage: u8,
    /// `D_800C2FE0`.
    pub last_stage: u8,
    /// `D_80069658/C`: kept (and added to) by the set match.
    pub scores: [u32; 2],
    /// `D_800B138C[p]`: all clears, kept like the scores.
    pub all_clears: [u16; 2],
    /// `D_8010ADA8`: the AI's idle countdown, which nothing resets.
    pub wiggle: i16,
}

/// The stage-won tally (`func_8003D2AC`, `func_8003D388`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bonus {
    /// `D_800B1388`: 10 × (best chain + average chain), 16-bit.
    pub chain: i32,
    /// `D_800C3400`: what is left to add to the score.
    pub points: u32,
    /// Frames of play (`D_80066AD0`), shown as the clear time.
    pub frames: u32,
}

impl Bonus {
    fn new(g: &crate::game::Game) -> Bonus {
        let s = &g.stats;
        let avg = if s.chain_turns == 0 { 0 } else { (s.chain_sum / s.chain_turns) as i32 };
        let chain = i32::from((i32::from(s.best_chain) * 10 + avg * 10) as i16);
        let frames = g.play_frames();
        let time = match frames {
            0..=1199 => (1200 - frames) / 6 + 1000,
            1200..=7199 => 1000,
            _ => 0,
        };
        Bonus { chain, points: time.wrapping_add(chain as u32).wrapping_mul(100), frames }
    }

    /// `func_8003D448`: index into [`RANKS`] (unsigned compare, so a negative value is "SPECIAL A").
    pub fn rank(&self) -> usize {
        RANK_ABOVE.iter().position(|&t| t < self.chain as u32).unwrap_or(5)
    }
}

#[derive(Clone, Debug)]
pub struct VsCpu {
    pub versus: Versus,
    pub sub: Sub,
    /// The menu pad: the human's player.
    pub me: usize,
    /// `D_800C30C8`: 0 Beginner, 1 Normal, 2 Hard.
    pub course: u8,
    /// `D_800662F0`.
    pub stage: u8,
    /// `D_800C2FE0`: winning brings `stage` to this and clears the course.
    pub last_stage: u8,
    /// `D_80069658/C`: the scores, carried from stage to stage.
    pub scores: [u32; 2],
    /// `D_800B138C[p]`: all clears, carried like the scores (only mode 5's entry clears them).
    pub all_clears: [u16; 2],
    pub winner: Option<usize>,
    pub roulette: [Roulette; 2],
    pub tallies: [[ColumnTally; 6]; 2],
    pub pause: Pause,
    pub over: GameOver,
    pub bonus: Bonus,
    /// `D_801026D8` / `D_801003DC` and `D_80066ACC` / `D_8010324C`.
    pub phase: u8,
    pub timer: u32,
    /// `D_8010ADA8`: the AI's idle countdown, never reset.
    wiggle: i16,
    pub exit: Option<Exit>,
    pub sounds: Vec<u8>,
    pub scripts: Vec<u8>,
    /// Scripts requested as `0x2xx` (20-byte records with start positions, `func_8002EF44`).
    pub placed: Vec<u8>,
    prev: [u16; 2],
    pub frame: u64,
    /// The CPU side's mascot (`func_8003121C`; only the actor of the AI's player is enabled).
    pub mascot: Mascot,
    /// Set by the renderer each frame: the actor script's done flag (`D_8005F3D4/8`).
    pub mascot_done: bool,
    /// The music director (`func_80039EE4`, mode 13).
    pub bgm: Director,
    /// This frame's music request.
    pub music: Option<Request>,
}

impl VsCpu {
    /// Launch a course (`func_8002922C` → mode 13): `stage` and `last_stage` from
    /// [`crate::front::arcade::VS_COURSES`].
    pub fn new(seed: u32, me: usize, course: u8, stage: u8, last_stage: u8) -> VsCpu {
        let c = Course { menu_pad: me, course, stage, last_stage, scores: [0; 2], all_clears: [0; 2], wiggle: 0 };
        VsCpu::resume(seed, c)
    }

    /// Back from mode 15 (`func_8002D114` state 5 with `D_800B137C` = 1): sub-state 0 on the
    /// interrupted stage, which loads its art again (`D_80100688` = stage − 1), with the
    /// match winner on the menu pad.
    pub fn resume(seed: u32, c: Course) -> VsCpu {
        let Course { menu_pad: me, course, stage, last_stage, scores, all_clears, wiggle } = c;
        let mut m = VsCpu {
            versus: Versus::vs_cpu(seed, course, stage.into()),
            sub: Sub::Load,
            me: me & 1,
            course,
            stage,
            last_stage,
            scores,
            all_clears,
            winner: None,
            roulette: [Roulette::new(); 2],
            tallies: Default::default(),
            pause: Pause::default(),
            over: GameOver::default(),
            bonus: Bonus::default(),
            phase: 0,
            timer: 0,
            wiggle,
            exit: None,
            sounds: Vec::new(),
            scripts: Vec::new(),
            placed: Vec::new(),
            prev: [0; 2],
            frame: 0,
            mascot: Mascot::versus(),
            mascot_done: false,
            bgm: Director::new(Kind::VsCpu),
            music: None,
        };
        m.init(seed, true);
        m
    }

    fn cpu_pad(&self) -> usize {
        1 - self.me
    }

    /// Sub-state 0 (`func_80039CEC`, `func_8001FC64`, `func_80034F74`, …): fresh fields on the
    /// stage's row, the AI's record for it. A new stage loads its art first (sub-state 11).
    fn init(&mut self, seed: u32, new_stage: bool) {
        if let Some(cpu) = &self.versus.cpu[self.cpu_pad()] {
            self.wiggle = cpu.wiggle;
        }
        let (_, variant) = crate::tables::vs_cpu_levels(self.course);
        self.versus = Versus::vs_cpu(seed, self.course, self.stage.into());
        self.bgm = Director::new(Kind::VsCpu);
        let cpu = Cpu::new(Params::stage(variant, self.stage.min(13).into()), self.wiggle);
        self.versus.cpu[self.cpu_pad()] = Some(cpu);
        for (g, (&s, &n)) in self.versus.games.iter_mut().zip(self.scores.iter().zip(&self.all_clears)) {
            g.hud.score = s;
            g.all_clears = n;
        }
        (self.roulette, self.tallies) = crate::roulette::new_strip(&mut self.versus);
        self.winner = None;
        self.pause = Pause::default();
        self.over = GameOver::default();
        self.bonus = Bonus::default();
        self.phase = 0;
        self.timer = 0;
        self.scripts.push(script::READY);
        // Sub-state 0: D_8010AE00 = 0, so func_8003121C re-inits (func_800319CC).
        // (The restart count keeps rising so a renderer sees the new stage's actor start.)
        let starts = self.mascot.starts + 1;
        self.mascot = Mascot::versus();
        self.mascot.starts = starts;
        self.sub = if new_stage { Sub::Load } else { Sub::Play };
    }


    /// The course as mode 15 leaves it.
    pub fn course_state(&self) -> Course {
        let wiggle = self.versus.cpu[self.cpu_pad()].as_ref().map_or(self.wiggle, |c| c.wiggle);
        Course {
            menu_pad: self.me,
            course: self.course,
            stage: self.stage,
            last_stage: self.last_stage,
            scores: self.scores,
            all_clears: self.all_clears,
            wiggle,
        }
    }

    fn save_scores(&mut self) {
        for (p, g) in self.versus.games.iter().enumerate() {
            self.scores[p] = g.hud.score;
            self.all_clears[p] = g.all_clears;
        }
    }


    /// One 60 Hz frame with both pads' held buttons (the AI replaces its pad's).
    pub fn step(&mut self, held: [u16; 2]) {
        let pressed = [held[0] & !self.prev[0], held[1] & !self.prev[1]];
        self.prev = held;
        self.sounds.clear();
        self.scripts.clear();
        self.placed.clear();
        self.frame += 1;
        if self.exit.is_some() {
            self.music = None;
            return;
        }
        // func_80014634 runs the director before the mode handler; D_80069694 = loser + 1.
        let ended = self.winner.map_or(0, |w| (2 - w) as u8);
        self.music = self.bgm.step(&Inputs::versus(&self.versus, self.sub == Sub::Play, ended, self.me));
        match self.sub {
            Sub::Load => {
                self.timer += 1;
                if self.timer > 2 {
                    self.timer = 0;
                    self.scripts.push(script::READY);
                    self.sub = Sub::Play;
                }
            }
            Sub::Play => self.play(held, pressed),
            Sub::Paused => {
                self.step_mascot();
                let mut sounds = Vec::new();
                self.pause.step(pressed[self.me], |id| sounds.push(id));
                self.sounds = sounds;
                self.step_spans();
                match self.pause.outcome {
                    Some(_) => self.exit = Some(Exit::Title),
                    None if !self.pause.active => self.sub = Sub::Play,
                    None => {}
                }
            }
            Sub::Won => {
                self.won(pressed[self.me]);
                self.step_mascot();
            }
            Sub::Lost => {
                self.lost(pressed[self.me]);
                self.step_mascot();
            }
            Sub::Challenger => {
                self.step_mascot();
                self.timer += 1;
                self.step_spans();
                if pressed[self.me] == pad::START || self.timer >= CHALLENGER_FRAMES {
                    self.exit = Some(Exit::Challenger(self.course_state()));
                }
            }
        }
    }

    /// `func_8003121C` for the AI's actor (`D_800699D2` when the human is player 1's pad 0,
    /// else `D_800699D0`).
    fn step_mascot(&mut self) {
        let p = self.cpu_pad();
        let g = &self.versus.games[p];
        let cues = VersusCues {
            chain: g.stats.chain,
            dropped: g.garbage.dropped,
            blobs: g.field.blob_count(),
            opponent_blobs: self.versus.games[1 - p].field.blob_count(),
            // D_80069694 is set by the end check, after this call on that frame.
            lost: self.winner.map(|w| w != p),
            done: self.mascot_done,
        };
        self.mascot.versus_step(cues);
    }

    fn step_spans(&mut self) {
        for g in &mut self.versus.games {
            g.field.step_span_segments();
        }
    }

    /// Sub-state 1.
    fn play(&mut self, held: [u16; 2], pressed: [u16; 2]) {
        self.versus.step(held);
        let mut sounds = Vec::new();
        for g in &self.versus.games {
            for &id in &g.sounds {
                if !sounds.contains(&id) {
                    sounds.push(id);
                }
            }
        }
        // func_8003BCD0, as in mode 14.
        crate::roulette::step_strip(&mut self.roulette, &mut self.tallies, &mut self.versus, pressed, &mut sounds);
        self.sounds = sounds;
        self.step_mascot();
        if self.versus.games[0].clock_running() && pressed[self.me] == pad::START {
            self.pause.begin_versus();
            self.sub = Sub::Paused;
        } else if pressed[self.cpu_pad()] == pad::START {
            // The challenger check runs whether or not the clock is going.
            self.sub = Sub::Challenger;
            self.timer = 0;
            self.save_scores();
            self.scripts.push(script::CHALLENGER);
            self.sounds.push(se::CHALLENGER);
            self.music = Some(Request::Xa(crate::bgm::xa::CHALLENGER));
            return;
        }
        self.end_check();
    }

    /// `func_8003CDAC` → `func_8002DD28` (both out on one frame counts as player 1's win).
    fn end_check(&mut self) {
        let Some(winner) = self.versus.set_winner() else { return };
        self.winner = Some(winner);
        crate::roulette::stop_spins(&mut self.roulette);
        self.phase = 0;
        self.timer = 0;
        // func_8003B2B8.
        self.versus.finish(winner);
        self.sounds.push(se::ROUND_END);
        self.save_scores();
        if winner == self.me {
            self.stage += 1;
            self.sub = Sub::Won;
        } else {
            // func_8002D0C4.
            self.sub = Sub::Lost;
            self.over.begin(false);
            self.over.limit = LOST_WAIT;
        }
    }

    /// `func_8003B03C`, modes 13–15.
    fn step_end_fields(&mut self) {
        let Some(w) = self.winner else { return };
        self.versus.games[1 - w].field.burst_step();
        let popped = self.versus.games[w].field.pop_special_step();
        // func_8003B644 always takes them off player 1's grey count.
        let f = &mut self.versus.games[1].field;
        f.grey = f.grey.wrapping_sub(popped);
    }

    /// Whether the stage just won was the course's last (`D_800B004C`).
    pub fn cleared(&self) -> bool {
        self.stage == self.last_stage
    }

    /// Phase ≥ 3: the rank, time and bonus are on screen.
    pub fn results_shown(&self) -> bool {
        self.sub == Sub::Won && self.phase >= 3
    }

    /// Sub-state 2 (`func_8003CF2C`).
    fn won(&mut self, pressed: u16) {
        self.timer += 1;
        self.step_spans();
        let skip = pressed == pad::CROSS || pressed == pad::START;
        let me = self.me;
        match self.phase {
            0 => {
                self.placed.push(script::WON + me as u8);
                self.phase = 1;
            }
            1 => {
                self.bonus = Bonus::new(&self.versus.games[me]);
                self.step_end_fields();
                if self.timer >= WON_TALLY_UNTIL {
                    self.phase = 2;
                }
            }
            2 => {
                if self.timer >= WON_SHOW_AT {
                    self.phase = 3;
                }
            }
            3 => {
                if self.timer >= WON_COUNT_AT {
                    self.phase = 4;
                }
            }
            4 => {
                // func_8003D7A4.
                let b = &mut self.bonus.points;
                if *b > 1000 {
                    *b -= 1000;
                    self.scores[me] = self.scores[me].wrapping_add(1000);
                    self.sounds.push(se::TALLY);
                } else {
                    self.scores[me] = self.scores[me].wrapping_add(*b);
                    *b = 0;
                }
                if skip || self.timer >= WON_FLUSH_AT {
                    self.phase = 5;
                }
            }
            5 => {
                // func_8003D400.
                self.scores[me] = self.scores[me].wrapping_add(self.bonus.points);
                self.bonus.points = 0;
                if skip || self.timer >= WON_NEXT_AT {
                    self.phase = 6;
                }
            }
            _ => {
                if self.cleared() {
                    self.exit = Some(Exit::Ending);
                    return;
                }
                let seed = self.versus.seed();
                self.init(seed, true);
            }
        }
        for (g, (&s, &n)) in self.versus.games.iter_mut().zip(self.scores.iter().zip(&self.all_clears)) {
            g.hud.score = s;
            g.all_clears = n;
        }
    }

    /// Sub-state 4 (`func_8002D114`, mode 13): no emblem, 300 frames, then the countdown.
    fn lost(&mut self, pressed: u16) {
        let mut sounds = Vec::new();
        let first = self.over.phase == crate::over::phase::RESULT;
        let moving = self.over.step(pressed, |id| sounds.push(id));
        self.sounds.extend(sounds);
        self.scripts = self.over.scripts.clone();
        if let Some(n) = self.over.xa {
            self.music = Some(Request::Xa(n));
        }
        if first {
            // func_8002D91C, mode 13: D_800C3514 = 0.
            self.scripts.clear();
            self.over.message = 0;
        }
        if moving {
            self.step_end_fields();
        }
        self.step_spans();
        match self.over.outcome {
            Some(Outcome::Retry) => {
                // Continuing on a long course gives up the last opponent.
                if self.course != 0 && self.stage != CONTINUED_LAST {
                    self.last_stage = CONTINUED_LAST;
                }
                self.scores[self.me] = self.scores[self.me].wrapping_add(1);
                let seed = self.versus.seed();
                self.init(seed, false);
            }
            Some(Outcome::Title) => self.exit = Some(Exit::Title),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::ps;
    use crate::game::Game;

    #[test]
    fn hard_course_uses_its_table_and_records() {
        let m = VsCpu::new(5, 0, 2, 3, 14);
        let g = &m.versus.games[0];
        assert_eq!(g.level_info().quota, 40);
        assert_eq!(g.level_info().speed, 8);
        assert!(m.versus.cpu[1].is_some() && m.versus.cpu[0].is_none());
        assert_eq!(m.versus.games[0].queue, m.versus.games[1].queue);
    }

    #[test]
    fn bonus_and_rank() {
        let mut g = Game::new(1, 3);
        g.stats.best_chain = 5;
        g.stats.chain_turns = 4;
        g.stats.chain_sum = 10;
        // 10·5 + 10·2 = 70 → C; 70 + 1000 (time 0) → 107,000.
        let b = Bonus::new(&g);
        assert_eq!(b.chain, 70);
        assert_eq!(b.rank(), 3);
        assert_eq!(b.points, (1000 + 200 + 70) * 100);
        assert_eq!(Bonus::default().rank(), 5);
    }

    #[test]
    fn losing_offers_a_continue_and_keeps_the_stage() {
        let mut m = VsCpu::new(9, 0, 1, 3, 14);
        for _ in 0..3 {
            m.step([0; 2]);
        }
        assert_eq!(m.sub, Sub::Play);
        m.versus.games[0].player.state = ps::GAME_OVER;
        m.step([0; 2]);
        assert_eq!(m.sub, Sub::Lost);
        let mut t = 0;
        while m.over.phase != crate::over::phase::COUNTDOWN {
            m.step([0; 2]);
            t += 1;
            assert!(t < 400);
        }
        m.step([pad::START, 0]);
        for _ in 0..80 {
            m.step([0; 2]);
        }
        assert_eq!(m.sub, Sub::Play);
        assert_eq!(m.stage, 3);
        assert_eq!(m.last_stage, CONTINUED_LAST);
        assert_eq!(m.scores[0], 1);
    }

    #[test]
    fn winning_the_last_stage_ends_the_course() {
        let mut m = VsCpu::new(9, 1, 0, 2, 3);
        m.step([0; 2]);
        m.step([0; 2]);
        m.step([0; 2]);
        m.versus.games[0].player.state = ps::GAME_OVER;
        m.step([0; 2]);
        assert_eq!(m.sub, Sub::Won);
        assert!(m.cleared());
        for _ in 0..WON_NEXT_AT + 2 {
            m.step([0; 2]);
        }
        assert_eq!(m.exit, Some(Exit::Ending));
        assert!(m.scores[1] >= 100_000);
    }

    #[test]
    fn a_cpu_plays_a_whole_stage() {
        let mut m = VsCpu::new(3, 0, 0, 0, 3);
        for _ in 0..60 * 120 {
            m.step([0; 2]);
            if m.sub != Sub::Play && m.sub != Sub::Load {
                break;
            }
        }
        // The idle human loses to the AI.
        assert_eq!(m.sub, Sub::Lost);
    }
}
