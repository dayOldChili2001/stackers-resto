//! Arcade's two-player set match (mode 14, `func_80015FCC`).
//!
//! Sub-states (`D_800C34AA`, `jtbl_80010118`): 0 init, 1 play, 2 set result
//! (`func_8003D184`), 3 pause (`func_8002E59C`), 4 match end (`func_8002D114`), 12 the in-game
//! difficulty re-pick (`func_8002B8EC`). The first to win `sets / 2 + 1` sets wins the match.
//! Each player has a difficulty (`D_80102740`, row of `D_800120D4`) and a bomb stock
//! (`D_800FFCF8`, copied to `D_80100674` at the start of every set).
//!
//! Mode 15 is the same match started by a challenger in vs CPU ([`SetMatch::challenger`]):
//! difficulty 3 vs 3, no bombs, 3 sets, no re-pick. The loser's continue is a rematch with the
//! winner on the menu pad; when the countdown runs out the winner goes back to the course.

use crate::bgm::{Director, Inputs, Kind, Request};
use crate::game::pad;
use crate::over::{GameOver, Outcome};
use crate::pause::Pause;
use crate::roulette::{ColumnTally, Roulette};
use crate::versus::Versus;
use crate::vs_cpu::Course;

/// `SD_CHR.OBJ` script ids (`D_800C3514`).
pub mod script {
    /// "Ready, Go!" at the start of every set (`0x100`).
    pub const READY: u8 = 0;
    /// Set won by player 2 / player 1 (`func_8002D91C`, `D_800B1238` 1 / 0).
    pub const SET_P2: u8 = 0x16;
    pub const SET_P1: u8 = 0x17;
    /// Match won by player 2 / player 1.
    pub const MATCH_P2: u8 = 0x45;
    pub const MATCH_P1: u8 = 0x46;
    /// The re-pick screen opens / both players confirmed (`func_8002B8EC`).
    pub const REPICK: u8 = 0x42;
    pub const REPICK_DONE: u8 = 0x43;
}

/// Set-result timings (`D_80066ACC`, counted from the end of the set): the result appears at
/// 0x5B, the loser can skip from 0xC9, and the next set starts at 0x169 at the latest.
pub const SET_RESULT_AT: u32 = 0x5B;
pub const SET_SKIP_AT: u32 = 0xC9;
pub const SET_NEXT_AT: u32 = 0x169;
/// Most bombs a player can stock (the Arcade list's ← / →).
pub const MAX_BOMBS: u8 = 3;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sub {
    /// Sub-state 0: the next set's init (`func_80039DE8` …) runs on the frame after the set
    /// result, the rematch or the re-pick asks for it.
    Init,
    Play,
    Paused,
    SetResult,
    MatchEnd,
    Repick,
}

/// `D_80100368`: the re-pick arrows' y for each difficulty (the mode menu's list positions).
pub const REPICK_ARROW_Y: [i32; 5] = [0x43, 0x63, 0x83, 0xA3, 0xC3];
/// `D_800B1344` / `D_800B1368` on entry: player 1's arrow at the left, player 2's (mirrored)
/// at the right.
pub const REPICK_ARROW_X: [i32; 2] = [0x28, 0xD8];
/// Frames of `D_80056A88` an arrow plays after a confirm (then it rests on frame 0).
const REPICK_ARROW_FRAMES: usize = 0x1B;
/// Re-pick states (`D_800699C0`).
pub mod repick {
    /// Script 0x42, everything reset; the SD VM doesn't run this frame.
    pub const INIT: u8 = 0;
    /// The lists slide in (0x14 frames, no arrows).
    pub const SLIDE_IN: u8 = 1;
    /// Both players pick.
    pub const PICK: u8 = 2;
    /// Both ready: script 0x43, the arrows fly apart after 0x14 frames.
    pub const OUT: u8 = 3;
}

/// One player's column of the re-pick screen (`D_800B1400[p]`, `D_800B1338 + 0x24·p` …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repick {
    /// 0–4: difficulty − 1.
    pub cursor: u8,
    /// `D_800B1340`: 0 choosing, 2 just confirmed (the arrow plays), 3 ready (△ goes back).
    pub state: u8,
    /// `D_800B1338`: the arrow's frame in `D_80056A88` (0 at rest).
    pub arrow_frame: usize,
    /// `D_800B1344/8`: the arrow's position.
    pub arrow: (i32, i32),
}

impl Repick {
    fn new(p: usize) -> Repick {
        Repick { cursor: 2, state: 0, arrow_frame: 0, arrow: (REPICK_ARROW_X[p], REPICK_ARROW_Y[2]) }
    }
}

impl Default for Repick {
    fn default() -> Self {
        Repick::new(0)
    }
}

/// `D_80103658[0..11]` on entry: the list's item palettes (slot i = difficulty i + 1; 5–10 = 6).
const REPICK_PALETTES: [i16; 11] = [0, 0, 0, 0, 0, 6, 6, 6, 6, 6, 6];

#[derive(Clone, Debug)]
pub struct SetMatch {
    pub versus: Versus,
    pub sub: Sub,
    /// `D_80102740`: difficulty 1–5 per player.
    pub difficulty: [u8; 2],
    /// `D_800FFCF8`: bombs per set.
    pub bombs: [u8; 2],
    /// Sets in the match (1, 3 or 5).
    pub sets: u8,
    /// `D_8010ADFC/E`: sets won this match.
    pub wins: [u8; 2],
    /// `D_800699C4/6`: matches won since power-on (capped at 999).
    pub tally: [u16; 2],
    /// The winner of the last set (`D_800B1238` = 1 when player 2 won).
    pub winner: Option<usize>,
    pub roulette: [Roulette; 2],
    /// The incoming-garbage digits over columns 1–6.
    pub tallies: [[ColumnTally; 6]; 2],
    pub pause: Pause,
    /// `D_8010ADAE`: the pad that paused (only it drives the menu).
    pub paused_by: usize,
    pub over: GameOver,
    /// `D_801026D8` and `D_80066ACC` of the set result.
    pub phase: u8,
    pub timer: u32,
    pub repick: [Repick; 2],
    /// `D_80103658`: the re-pick list's item palettes (SD draw mode 1).
    pub repick_palettes: [i16; 11],
    /// The SD VM runs this frame in the re-pick (`func_8002EAD4`; not on its first frame).
    pub run_sd: bool,
    /// `D_80069658/C`: the scores, kept from set to set (only Arcade's entry clears them).
    pub scores: [u32; 2],
    /// `D_800B138C[p]`: all clears, kept like the scores.
    pub all_clears: [u16; 2],
    /// `D_800C2FB8`: the pad that started the mode (its score gets the continue point).
    pub menu_pad: usize,
    /// Mode 15: the vs CPU course this match interrupted.
    pub course: Option<Course>,
    /// Mode 15's countdown ran out: the course to resume, the winner on its menu pad.
    pub back: Option<Course>,
    /// Set when the run leaves the mode (Quit, or nobody continued).
    pub outcome: Option<Outcome>,
    pub sounds: Vec<u8>,
    pub scripts: Vec<u8>,
    /// Set by the caller before a step: which ports' connection state (`D_800B1440` /
    /// `D_800B1462`) differs from last frame's (`D_8006626C/D`, saved at the end of every
    /// frame). While the clock runs, a change opens the pause menu like Start. Taken by the step.
    pub port_change: [bool; 2],
    prev: [u16; 2],
    pub frame: u64,
    /// The music director (`func_80039EE4`, modes 14 and 15).
    pub bgm: Director,
    /// This frame's music request.
    pub music: Option<Request>,
}

impl SetMatch {
    pub fn new(seed: u32, difficulty: [u8; 2], bombs: [u8; 2], sets: u8) -> SetMatch {
        SetMatch::start(seed, difficulty, bombs, sets, None)
    }

    /// Mode 15 (`func_80016480` sub-state 10): `D_80102740/2` = 3, no bombs, 3 sets, stage 0's
    /// SD images, `D_800B137C` = 1.
    pub fn challenger(seed: u32, course: Course) -> SetMatch {
        SetMatch::start(seed, [3, 3], [0, 0], 3, Some(course))
    }

    fn start(seed: u32, difficulty: [u8; 2], bombs: [u8; 2], sets: u8, course: Option<Course>) -> SetMatch {
        let mut m = SetMatch {
            versus: Versus::set_match(seed, difficulty, bombs),
            sub: Sub::Play,
            difficulty,
            bombs,
            sets: sets.max(1),
            wins: [0; 2],
            tally: [0; 2],
            winner: None,
            roulette: [Roulette::new(); 2],
            tallies: [[ColumnTally::default(); 6]; 2],
            pause: Pause::default(),
            paused_by: 0,
            over: GameOver::default(),
            phase: 0,
            timer: 0,
            repick: [Repick::new(0), Repick::new(1)],
            repick_palettes: REPICK_PALETTES,
            run_sd: false,
            scores: course.map_or([0; 2], |c| c.scores),
            all_clears: course.map_or([0; 2], |c| c.all_clears),
            menu_pad: course.map_or(0, |c| c.menu_pad),
            course,
            back: None,
            outcome: None,
            sounds: Vec::new(),
            scripts: Vec::new(),
            port_change: [false; 2],
            prev: [0; 2],
            frame: 0,
            bgm: Director::new(Kind::SetMatch),
            music: None,
        };
        if let Some(c) = course {
            for (g, &c) in m.versus.games.iter_mut().zip(&c.carry) {
                g.carry_in(c);
            }
        }
        m.start_set(seed);
        m
    }

    /// Sub-state 0: fresh fields, levels and stock; the HUD indicators back to column 2.
    fn start_set(&mut self, seed: u32) {
        let bombs = self.bombs.map(|b| b.min(MAX_BOMBS));
        // The init leaves a few of the players' values as the last set ended them.
        let old = std::mem::replace(&mut self.versus, Versus::set_match(seed, self.difficulty, bombs));
        for (g, o) in self.versus.games.iter_mut().zip(&old.games) {
            g.carry_from(o);
        }
        // The shake's words are globals (D_80100408 …), untouched by the init.
        self.versus.shake = old.shake;
        self.bgm = Director::new(if self.course.is_some() { Kind::Challenger } else { Kind::SetMatch });
        for (g, (&s, &n)) in self.versus.games.iter_mut().zip(self.scores.iter().zip(&self.all_clears)) {
            g.hud.score = s;
            g.all_clears = n;
            // func_8003AE70 tests D_80100684 == 14.
            g.divisor_ahead = self.course.is_none();
        }
        (self.roulette, self.tallies) = crate::roulette::new_strip(&mut self.versus);
        self.sub = Sub::Play;
        self.winner = None;
        // The pause menu's state isn't reset (func_8002E514 does that when it opens).
        self.over = GameOver::default();
        self.phase = 0;
        self.timer = 0;
        self.scripts.push(script::READY);
    }

    /// The pad that answers the result screens: the loser's (`(D_800B1238 + 1) & 1`).
    fn loser_pad(&self) -> usize {
        1 - self.winner.unwrap_or(0)
    }


    /// One 60 Hz frame with both pads' held buttons.
    pub fn step(&mut self, held: [u16; 2]) {
        let pressed = [held[0] & !self.prev[0], held[1] & !self.prev[1]];
        let ports = std::mem::take(&mut self.port_change);
        self.prev = held;
        self.sounds.clear();
        self.scripts.clear();
        self.frame += 1;
        // Only sub-states 1, 3, 4 and 12 draw the frame (func_8001A82C, with the shake).
        self.versus.shake.y = 0;
        if self.outcome.is_some() {
            self.music = None;
            return;
        }
        // func_80014634 runs the director before the mode handler; D_80069694 = loser + 1.
        let ended = self.winner.map_or(0, |w| (2 - w) as u8);
        self.music = self.bgm.step(&Inputs::versus(&self.versus, self.sub == Sub::Play, ended, self.menu_pad));
        match self.sub {
            Sub::Init => {
                let seed = self.versus.seed();
                self.start_set(seed);
            }
            Sub::Play => self.play(held, pressed, ports),
            Sub::Paused => {
                self.versus.hold(held);
                let mut sounds = Vec::new();
                self.pause.step_versus(pressed[self.paused_by], |id| sounds.push(id));
                // The menu resumed: the strip's logic runs this frame (sub-state 1 already).
                if !self.pause.active && self.pause.outcome.is_none() {
                    crate::roulette::step_strip(&mut self.roulette, &mut self.tallies, &mut self.versus, pressed, &mut sounds);
                }
                // func_80015FCC sub-state 3: Start alone on either pad resumes (after the strip).
                if pressed.contains(&pad::START) {
                    self.pause.resume();
                }
                self.sounds = sounds;
                self.versus.step_shake(&mut self.sounds);
                self.step_spans();
                match self.pause.outcome {
                    Some(o) => self.outcome = Some(o),
                    None if !self.pause.active => self.sub = Sub::Play,
                    None => {}
                }
            }
            Sub::SetResult => self.set_result(pressed),
            Sub::MatchEnd => {
                self.match_end(pressed);
                self.versus.step_shake(&mut self.sounds);
            }
            Sub::Repick => {
                self.step_repick(pressed);
                self.versus.step_shake(&mut self.sounds);
            }
        }
    }

    fn step_spans(&mut self) {
        for g in &mut self.versus.games {
            g.field.step_span_segments();
        }
    }

    /// Sub-state 1.
    fn play(&mut self, held: [u16; 2], pressed: [u16; 2], ports: [bool; 2]) {
        self.versus.step(held);
        let mut sounds = Vec::new();
        for g in &self.versus.games {
            for &id in &g.sounds {
                if !sounds.contains(&id) {
                    sounds.push(id);
                }
            }
        }
        // func_8003BCD0: the special-column indicators and the incoming-garbage digits.
        crate::roulette::step_strip(&mut self.roulette, &mut self.tallies, &mut self.versus, pressed, &mut sounds);
        self.sounds = sounds;
        // func_8001A82C.
        self.versus.step_shake(&mut self.sounds);
        let running = self.versus.games[0].clock_running();
        if running {
            if let Some(p) = (0..2).find(|&p| pressed[p] == pad::START) {
                self.paused_by = p;
                self.pause.begin_versus();
                self.sub = Sub::Paused;
            }
            // func_80015FCC then checks the ports: a change pauses with that pad's menu
            // (`D_8010ADAE`), port 1 first, overriding the Start check's pad.
            if let Some(p) = (0..2).find(|&p| ports[p]) {
                self.paused_by = p;
                self.pause.begin_versus();
                self.sub = Sub::Paused;
            }
        }
        self.end_check();
    }

    /// `func_8003CBAC` → `func_8002DD28`: a player in state 10 loses the set; both on one frame
    /// counts as player 1's win (result 2).
    fn end_check(&mut self) {
        let Some(winner) = self.versus.set_winner() else { return };
        self.winner = Some(winner);
        self.wins[winner] += 1;
        for (p, g) in self.versus.games.iter().enumerate() {
            self.scores[p] = g.hud.score;
            self.all_clears[p] = g.all_clears;
        }
        crate::roulette::stop_spins(&mut self.roulette);
        self.sub = Sub::SetResult;
        self.phase = 0;
        self.timer = 0;
        // func_8003B2B8, modes 13–15: the loser bursts, the winner's blobs pop.
        self.versus.finish(winner);
        self.sounds.push(0x1E);
        if self.wins[winner] >= self.sets / 2 + 1 {
            self.tally[winner] = (self.tally[winner] + 1).min(999);
            // func_8002D0C4: straight to sub-state 4.
            self.sub = Sub::MatchEnd;
            self.over.begin(false, false);
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

    /// Sub-state 2 (`func_8003D184`).
    fn set_result(&mut self, pressed: [u16; 2]) {
        self.timer += 1;
        self.step_spans();
        match self.phase {
            0 => {
                self.step_end_fields();
                if self.timer >= SET_RESULT_AT {
                    self.scripts.push(if self.winner == Some(1) { script::SET_P2 } else { script::SET_P1 });
                    self.phase = 1;
                }
            }
            1 => {
                if self.timer >= SET_SKIP_AT {
                    self.phase = 2;
                }
            }
            _ => {
                let p = pressed[self.loser_pad()];
                if p == pad::CROSS || p == pad::START || self.timer >= SET_NEXT_AT {
                    self.sub = Sub::Init;
                }
            }
        }
    }

    /// Sub-state 4 (`func_8002D114`): the result emblem, then the loser's continue countdown.
    fn match_end(&mut self, pressed: [u16; 2]) {
        let mut sounds = Vec::new();
        let first = self.over.phase == crate::over::phase::RESULT;
        let moving = self.over.step(pressed[self.loser_pad()], |id| sounds.push(id));
        self.sounds.extend(sounds);
        self.scripts = self.over.scripts.clone();
        if let Some(n) = self.over.xa {
            self.music = Some(Request::Xa(n));
        }
        if first {
            // func_8002D91C, mode 14.
            self.scripts = vec![if self.winner == Some(1) { script::MATCH_P2 } else { script::MATCH_P1 }];
            self.over.message = self.scripts[0];
        }
        if moving {
            self.step_end_fields();
        }
        self.step_spans();
        let winner = self.winner.unwrap_or(0);
        if self.over.outcome.is_some() {
            // Phases 4 and 5 end with func_80040A7C(1, 2): the music stops.
            self.music = Some(Request::Stop);
        }
        match self.over.outcome {
            // Phase 4: wins reset (the score + 1 is mode 13's alone); mode 14 goes to the
            // re-pick, mode 15 starts over (sub-state 0) with the winner on the menu pad and the
            // loser's tally cleared.
            Some(Outcome::Retry) => {
                self.wins = [0; 2];
                if let Some(c) = &mut self.course {
                    self.menu_pad = winner;
                    c.menu_pad = winner;
                    self.tally[1 - winner] = 0;
                    self.sub = Sub::Init;
                } else {
                    self.sub = Sub::Repick;
                    self.phase = repick::INIT;
                    self.timer = 0;
                }
            }
            // Phase 5 after 0xC8 frames: mode 15 returns to mode 13 (`D_800B137C` = 1) with the
            // winner as the menu pad (`D_800699C4[loser]` = 0); otherwise the title.
            Some(Outcome::Title) => {
                if let Some(c) = self.course {
                    self.tally[1 - winner] = 0;
                    let carry = [self.versus.games[0].carry(), self.versus.games[1].carry()];
                    self.back = Some(Course { menu_pad: winner, scores: self.scores, all_clears: self.all_clears, carry, ..c });
                }
                self.outcome = Some(Outcome::Title);
            }
            None => {}
        }
    }

    /// Sub-state 12 (`func_8002B8EC`): both players pick a difficulty (↑/↓) and bombs (←/→),
    /// ✕ or Start to confirm, △ to take it back; once both are ready the next match starts.
    fn step_repick(&mut self, pressed: [u16; 2]) {
        self.timer += 1;
        self.run_sd = true;
        match self.phase {
            repick::INIT => {
                self.scripts.push(script::REPICK);
                self.phase = repick::SLIDE_IN;
                self.repick = [Repick::new(0), Repick::new(1)];
                self.repick_palettes = REPICK_PALETTES;
                self.timer = 0;
                self.bombs = [0; 2];
                // Returns before the VM.
                self.run_sd = false;
            }
            repick::SLIDE_IN => {
                if self.timer == 0x14 {
                    self.phase = repick::PICK;
                    self.timer = 0;
                }
            }
            repick::PICK => self.repick_pick(pressed),
            _ => {
                // △ starts the re-pick over (state 0, no script until it starts 0x42 again).
                if pressed.contains(&pad::TRIANGLE) {
                    self.phase = repick::INIT;
                    self.timer = 0;
                }
                if self.timer >= 0x15 {
                    let [a, b] = &mut self.repick;
                    a.arrow.0 -= 0x10;
                    a.arrow.1 -= 1;
                    b.arrow.0 += 0x10;
                    b.arrow.1 -= 1;
                }
                if self.timer >= 0x50 {
                    self.sub = Sub::Init;
                }
            }
        }
    }

    /// Re-pick state 2.
    fn repick_pick(&mut self, mut pressed: [u16; 2]) {
        // △ on a ready player takes the choice back (and uses up the press). The item stays
        // lit if the other player is on it and not choosing.
        for p in 0..2 {
            if self.repick[p].state == 3 && pressed[p] == pad::TRIANGLE {
                self.repick[p].arrow_frame = 0;
                self.repick[p].state = 0;
                let other = self.repick[1 - p];
                if other.state == 0 || other.cursor != self.repick[p].cursor {
                    self.repick_palettes[usize::from(self.repick[p].cursor)] = 0;
                }
                pressed[p] = 0;
            }
        }
        for p in 0..2 {
            let b = pressed[p];
            let r = &mut self.repick[p];
            if r.state == 0 {
                if b == pad::UP {
                    r.cursor = if r.cursor == 0 { 4 } else { r.cursor - 1 };
                    self.sounds.push(crate::pause::SE_CHOOSE);
                }
                if b == pad::DOWN {
                    r.cursor = if r.cursor == 4 { 0 } else { r.cursor + 1 };
                    self.sounds.push(crate::pause::SE_CHOOSE);
                }
                if b == pad::RIGHT && self.bombs[p] < MAX_BOMBS {
                    self.bombs[p] += 1;
                    self.sounds.push(crate::pause::SE_CHOOSE);
                }
                if b == pad::LEFT && self.bombs[p] > 0 {
                    self.bombs[p] -= 1;
                    self.sounds.push(crate::pause::SE_CHOOSE);
                }
                if b == pad::START || b == pad::CROSS {
                    r.state = 1;
                }
            }
            if r.state == 1 {
                r.state = 2;
                self.sounds.push(0xA8);
                self.repick_palettes[usize::from(r.cursor)] = 1;
                self.difficulty[p] = r.cursor + 1;
            }
            if r.state == 2 {
                r.arrow_frame += 1;
                if r.arrow_frame >= REPICK_ARROW_FRAMES {
                    r.arrow_frame = 0;
                    r.state = 3;
                }
            }
            r.arrow.1 = REPICK_ARROW_Y[usize::from(r.cursor)];
            if self.repick.iter().all(|r| r.state == 3) && self.phase == repick::PICK {
                self.phase = repick::OUT;
                self.timer = 0;
                self.scripts.push(script::REPICK_DONE);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::game::ps;
    use crate::game::Rules;

    /// One line per frame for `tools/framediff.py match` / `cpu`: both players (as the solo dump),
    /// then each player's garbage counters and special column, then the shake's words.
    pub(crate) fn versus_line(v: &crate::versus::Versus) -> String {
        let mut s = String::new();
        for g in &v.games {
            let gb = &g.garbage;
            s += &format!(
                "{} # {:?} {:?} {} {} {} ## ",
                // The shared rand() lives in Versus, not in the games' copies.
                crate::game::tests::frame_line(g)
                    .replacen(&format!(" | {} |", g.rng.seed), &format!(" | {} |", v.seed()), 1),
                gb.incoming,
                gb.pending,
                g.special_column.map_or(-1, |c| c as i32 - 1),
                g.bombs,
                g.hud.score
            );
        }
        let k = &v.shake;
        s += &format!(
            "shake {} {} {} {} {} {} {} ## ",
            k.trigger, v.games[0].garbage.dropped, v.games[1].garbage.dropped, k.level, k.len, k.count, k.index
        );
        s
    }

    /// `FRAME_DUMP=/tmp/m.txt cargo test -p stackers-core set_match::tests::frame_dump -- --ignored`
    /// Random held pads on both sides (no Start) until the first set ends.
    #[test]
    #[ignore]
    fn frame_dump() {
        use crate::game::pad;
        use crate::rng::BiosRand;
        use std::fmt::Write as _;
        let path = std::env::var("FRAME_DUMP").expect("FRAME_DUMP");
        let games: u32 = std::env::var("FRAME_GAMES").map_or(10, |v| v.parse().unwrap());
        let mut rng = BiosRand::new(std::env::var("FRAME_SEED").map_or(777, |v| v.parse().unwrap()));
        let mut out = String::new();
        let buttons = [pad::LEFT, pad::RIGHT, pad::DOWN, pad::CROSS, pad::CIRCLE, pad::L1, pad::SQUARE];
        for _ in 0..games {
            let seed = rng.next() * 7919 + rng.next();
            let diff = [1 + (rng.next() % 5) as u8, 1 + (rng.next() % 5) as u8];
            let bombs = [(rng.next() % 4) as u8, (rng.next() % 4) as u8];
            let new = || SetMatch::new(seed, diff, bombs, 1);
            let mut m = new();
            let mut pads = Vec::new();
            let mut held = [0u16; 2];
            while m.sub == Sub::Play && pads.len() < 20_000 {
                for h in &mut held {
                    if rng.next() % 6 == 0 {
                        *h = 0;
                        for &b in &buttons {
                            // Bombs rarely, the rest often.
                            let odds = if b == pad::SQUARE { 40 } else { 4 };
                            if rng.next() % odds == 0 {
                                *h |= b;
                            }
                        }
                    }
                }
                pads.push(held);
                m.step(held);
            }
            write!(out, "M {seed} {} {} {} {} ", diff[0], diff[1], bombs[0], bombs[1]).unwrap();
            for h in &pads {
                write!(out, "{}/{},", h[0], h[1]).unwrap();
            }
            out.push('\n');
            let mut m = new();
            writeln!(out, "F {}", versus_line(&m.versus)).unwrap();
            for &h in &pads {
                m.step(h);
                writeln!(out, "F {}", versus_line(&m.versus)).unwrap();
            }
        }
        std::fs::write(path, out).unwrap();
    }

    /// `FRAME_DUMP=/tmp/p.txt cargo test -p stackers-core set_match::tests::pause_frame_dump -- --ignored`
    /// then `python3 tools/framediff.py matchpause /tmp/p.txt`: random held pads with Start on
    /// either side and the pause menu driven by both pads, until the first set ends or Quit.
    #[test]
    #[ignore]
    fn pause_frame_dump() {
        use crate::game::pad;
        use crate::rng::BiosRand;
        use std::fmt::Write as _;
        let path = std::env::var("FRAME_DUMP").expect("FRAME_DUMP");
        let games: u32 = std::env::var("FRAME_GAMES").map_or(10, |v| v.parse().unwrap());
        let mut rng = BiosRand::new(std::env::var("FRAME_SEED").map_or(31337, |v| v.parse().unwrap()));
        let mut out = String::new();
        let buttons = [pad::LEFT, pad::RIGHT, pad::DOWN, pad::CROSS, pad::CIRCLE, pad::L1, pad::SQUARE];
        let menu = [pad::UP, pad::DOWN, pad::DOWN, pad::CROSS, pad::CROSS, pad::TRIANGLE, pad::START];
        // FRAME_PORTS=<seed> adds port connection changes (a third pad field, bit 0 port 1,
        // bit 1 port 2) from their own rand, so the default dumps stay as they were.
        let mut prng = std::env::var("FRAME_PORTS").ok().map(|v| BiosRand::new(v.parse().unwrap()));
        let mut port = || prng.as_mut().map_or(0u8, |r| if r.next() % 400 == 0 { 1 + (r.next() % 3) as u8 } else { 0 });
        let mask = |c: u8| [c & 1 != 0, c & 2 != 0];
        for _ in 0..games {
            let seed = rng.next() * 7919 + rng.next();
            let diff = [1 + (rng.next() % 5) as u8, 1 + (rng.next() % 5) as u8];
            let bombs = [(rng.next() % 4) as u8, (rng.next() % 4) as u8];
            let new = || SetMatch::new(seed, diff, bombs, 1);
            let mut m = new();
            let mut pads = Vec::new();
            let mut ports = Vec::new();
            let mut held = [0u16; 2];
            let live = |m: &SetMatch| matches!(m.sub, Sub::Play | Sub::Paused) && m.outcome.is_none();
            while live(&m) && pads.len() < 20_000 {
                for (p, h) in held.iter_mut().enumerate() {
                    if m.sub == Sub::Paused {
                        // Both pads press things; only the one that paused counts (and either
                        // pad's Start).
                        let leaving = m.pause.menu == crate::pause::menu::QUIT;
                        let odds = if leaving { 120 } else if p == m.paused_by { 5 } else { 40 };
                        *h = if rng.next() % odds == 0 { menu[(rng.next() % menu.len() as u32) as usize] } else { 0 };
                        // Mostly back to play rather than Quit.
                        if *h == pad::CROSS && m.pause.cursor == 1 && rng.next() % 4 != 0 {
                            *h = pad::START;
                        }
                        // Some games quit.
                        if m.pause.menu == crate::pause::menu::CONFIRM && rng.next() % 12 == 0 {
                            *h = pad::CROSS;
                        }
                    } else if rng.next() % 700 == 0 {
                        *h = pad::START;
                    } else if *h == pad::START || rng.next() % 6 == 0 {
                        *h = 0;
                        for &b in &buttons {
                            let odds = if b == pad::SQUARE { 40 } else { 4 };
                            if rng.next() % odds == 0 {
                                *h |= b;
                            }
                        }
                        // Start held with other buttons doesn't pause (equality test).
                        if rng.next() % 50 == 0 {
                            *h |= pad::START;
                        }
                    }
                }
                pads.push(held);
                ports.push(port());
                m.port_change = mask(ports[ports.len() - 1]);
                m.step(held);
            }
            write!(out, "P {seed} {} {} {} {} ", diff[0], diff[1], bombs[0], bombs[1]).unwrap();
            for (h, &c) in pads.iter().zip(&ports) {
                if c == 0 {
                    write!(out, "{}/{},", h[0], h[1]).unwrap();
                } else {
                    write!(out, "{}/{}/{c},", h[0], h[1]).unwrap();
                }
            }
            out.push('\n');
            let mut m = new();
            writeln!(out, "F {}", pause_line(&m)).unwrap();
            for (&h, &c) in pads.iter().zip(&ports) {
                m.port_change = mask(c);
                m.step(h);
                writeln!(out, "F {}", pause_line(&m)).unwrap();
            }
        }
        std::fs::write(path, out).unwrap();
    }

    /// `FRAME_DUMP=/tmp/r.txt cargo test -p stackers-core set_match::tests::challenger_frame_dump -- --ignored`
    /// then `python3 tools/framediff.py challenger /tmp/r.txt`: mode 15 from vs CPU's sub-state
    /// 10, all three sets with their results, pauses, the countdown (rematches on the loser's
    /// Start) and the return to mode 13 or the title.
    #[test]
    #[ignore]
    fn challenger_frame_dump() {
        use crate::game::pad;
        use crate::rng::BiosRand;
        use std::fmt::Write as _;
        let path = std::env::var("FRAME_DUMP").expect("FRAME_DUMP");
        let games: u32 = std::env::var("FRAME_GAMES").map_or(8, |v| v.parse().unwrap());
        let mut rng = BiosRand::new(std::env::var("FRAME_SEED").map_or(1515, |v| v.parse().unwrap()));
        let mut out = String::new();
        let buttons = [pad::LEFT, pad::RIGHT, pad::DOWN, pad::CROSS, pad::CIRCLE, pad::L1];
        let menu = [pad::UP, pad::DOWN, pad::DOWN, pad::CROSS, pad::CROSS, pad::TRIANGLE, pad::START];
        for _ in 0..games {
            let seed = rng.next() * 7919 + rng.next();
            let course = Course {
                menu_pad: (rng.next() & 1) as usize,
                course: 1,
                stage: 3 + (rng.next() % 11) as u8,
                last_stage: 14,
                scores: [rng.next() * 3, rng.next() * 3],
                all_clears: [0; 2],
                wiggle: 0,
                // What vs CPU's last frame left (mode 13 → 15 keeps them).
                carry: [0; 2].map(|_| crate::game::Carry {
                    flip_cd: (rng.next() % 8) as i16,
                    move_delay: (rng.next() % 12) as i16,
                    drop_points: 2 * (rng.next() % 12) as u16,
                }),
            };
            let new = || SetMatch::challenger(seed, course);
            let mut m = new();
            let mut pads = Vec::new();
            let mut held = [0u16; 2];
            while m.outcome.is_none() && pads.len() < 60_000 {
                for (p, h) in held.iter_mut().enumerate() {
                    if m.sub == Sub::Paused {
                        let odds = if p == m.paused_by { 5 } else { 40 };
                        *h = if rng.next() % odds == 0 { menu[(rng.next() % menu.len() as u32) as usize] } else { 0 };
                        // Back to play, never Quit.
                        if *h == pad::CROSS && m.pause.cursor == 1 {
                            *h = pad::START;
                        }
                    } else if rng.next() % 1500 == 0 {
                        *h = pad::START;
                    } else if *h == pad::START || rng.next() % 6 == 0 {
                        *h = 0;
                        for &b in &buttons {
                            if rng.next() % 4 == 0 {
                                *h |= b;
                            }
                        }
                    }
                }
                pads.push(held);
                m.step(held);
            }
            let c = course;
            write!(out, "R {seed} {} {} {} {} ", c.menu_pad, c.stage, c.scores[0], c.scores[1]).unwrap();
            for k in c.carry {
                write!(out, "{},{},{} ", k.flip_cd, k.move_delay, k.drop_points).unwrap();
            }
            for h in &pads {
                write!(out, "{}/{},", h[0], h[1]).unwrap();
            }
            out.push('\n');
            let mut m = new();
            writeln!(out, "F {}", pause_line(&m)).unwrap();
            for &h in &pads {
                m.step(h);
                writeln!(out, "F {}", pause_line(&m)).unwrap();
            }
        }
        std::fs::write(path, out).unwrap();
    }

    /// [`versus_line`] plus the sub-state, the pause menu, who paused and the frame's sounds.
    fn pause_line(m: &SetMatch) -> String {
        let sub = match (m.outcome, m.sub) {
            (Some(_), _) => -1,
            (_, Sub::Play) => 1,
            (_, Sub::Paused) => 3,
            (_, Sub::Init) => 0,
            (_, Sub::SetResult) => 2,
            (_, Sub::MatchEnd) => 4,
            _ => -2,
        };
        let paused_by = if m.sub == Sub::Paused || m.outcome.is_some() { m.paused_by as i32 } else { -1 };
        pause_tail(&m.versus, &m.pause, sub, paused_by, &m.sounds)
    }

    /// [`versus_line`], then `#P` and the sub-state, the pause menu in `func_8002E79C`'s
    /// numbering, the pad that paused and the frame's sounds (sorted: the players' and the
    /// strip's calls interleave differently).
    pub(crate) fn pause_tail(v: &Versus, p: &Pause, sub: i32, paused_by: i32, sounds: &[u8]) -> String {
        let mut sounds = sounds.to_vec();
        sounds.sort_unstable();
        sounds.dedup();
        format!(
            "{}#P {sub} {} {} {} {paused_by} {:?}",
            versus_line(v),
            [0, 9, 1, 2, 3][usize::from(p.menu)],
            p.cursor,
            p.timer,
            sounds
        )
    }

    #[test]
    fn levels_come_from_the_difficulty_table() {
        let m = SetMatch::new(3, [1, 5], [0, 2], 3);
        let [a, b] = &m.versus.games;
        assert_eq!(a.rules, Rules::Versus);
        assert_eq!((a.level_info().colors, a.level_info().quota, a.speed_index), (4, 0x20, 3));
        assert_eq!((b.level_info().colors, b.level_info().quota, b.speed_index), (5, 0x10, 0xC));
        assert_eq!(b.garbage.pending[1..7], [5; 6]);
        assert_eq!(b.bombs, 2);
        assert!(!a.shared_ring);
        assert_eq!(a.special_column, Some(2));
    }

    #[test]
    fn a_bomb_waits_for_the_cut_in_then_excites_everything() {
        let mut m = SetMatch::new(3, [2, 2], [1, 0], 3);
        // Through the intro to the first piece.
        while m.versus.games[0].player.state != ps::CONTROL {
            m.step([0, 0]);
        }
        let g = &mut m.versus.games[0];
        g.field.put(2, 17, 1);
        g.field.put(3, 17, 0);
        m.step([pad::SQUARE, 0]);
        assert_eq!(m.versus.games[0].player.state, ps::BOMB);
        assert!(m.sounds.contains(&crate::game::se::BOMB));
        let mut frames = 1;
        while m.versus.games[0].player.state == ps::BOMB {
            m.step([0, 0]);
            frames += 1;
        }
        assert_eq!(frames, 46);
        let g = &m.versus.games[0];
        assert_eq!(g.bombs, 0);
        assert_eq!(g.player.state, ps::REACHING);
        // The coloured blob reaches out; the grey one doesn't.
        assert_ne!(g.field.cell(2, 17).state, crate::field::st::SETTLED);
        assert_eq!(g.field.cell(3, 17).state, crate::field::st::SETTLED);
        assert_eq!(g.field.cell(3, 17).color, 0);
    }

    #[test]
    fn clears_from_chain_2_start_cut_ins_and_a_bomb_waits_its_turn() {
        use crate::game::CutIn;
        let mut m = SetMatch::new(3, [2, 2], [1, 0], 3);
        let g = &mut m.versus.games[0];
        g.cleared_step = true;
        g.stats.chain = 1;
        g.step_cut_in();
        assert_eq!(g.cut_in, None);
        g.cleared_step = true;
        g.stats.chain = 17;
        g.bomb = 1;
        g.step_cut_in();
        assert_eq!((g.cut_in, g.bomb), (Some(CutIn { entry: 14, age: 0 }), 0xFF));
        g.bomb = 1;
        g.step_cut_in();
        assert_eq!((g.cut_in, g.bomb), (Some(CutIn { entry: 15, age: 0 }), 2));
        for _ in 0..44 {
            g.step_cut_in();
        }
        assert_eq!(g.bomb, 0xFF);
    }

    #[test]
    fn sets_decide_the_match() {
        let mut m = SetMatch::new(3, [2, 2], [0, 0], 3);
        m.versus.games[1].player.state = ps::GAME_OVER;
        m.end_check();
        assert_eq!((m.sub, m.winner, m.wins), (Sub::SetResult, Some(0), [1, 0]));
        let mut saw = Vec::new();
        for _ in 0..SET_SKIP_AT {
            m.step([0, 0]);
            saw.extend(m.scripts.clone());
        }
        assert_eq!(saw, [script::SET_P1]);
        // Player 1 (the winner) can't skip; player 2 can.
        m.step([pad::CROSS, 0]);
        assert_eq!(m.sub, Sub::SetResult);
        m.step([0, pad::CROSS]);
        // Sub-state 0 first: the init runs on the next frame.
        assert_eq!(m.sub, Sub::Init);
        m.step([0, 0]);
        assert_eq!(m.sub, Sub::Play);
        assert_eq!(m.wins, [1, 0]);
        m.versus.games[1].player.state = ps::GAME_OVER;
        m.end_check();
        assert_eq!((m.sub, m.tally), (Sub::MatchEnd, [1, 0]));
        m.step([0, 0]);
        assert_eq!(m.scripts, [script::MATCH_P1]);
    }

    #[test]
    fn the_spin_steps_every_5_frames_and_feeds_the_special_column() {
        let mut m = SetMatch::new(3, [2, 2], [0, 0], 1);
        m.roulette[0].start(1);
        for _ in 0..10 {
            m.roulette[0].step();
        }
        assert_eq!(m.roulette[0].column, 3);
        crate::roulette::sync_columns(&m.roulette, &mut m.versus);
        assert_eq!(m.versus.games[0].special_column, Some(4));
    }

    #[test]
    fn the_repick_slides_in_picks_and_flies_out_on_the_original_frames() {
        let mut m = SetMatch::new(3, [2, 2], [1, 1], 1);
        m.wins = [1, 0];
        m.sub = Sub::MatchEnd;
        m.over.begin(false, false);
        m.over.outcome = Some(Outcome::Retry);
        m.step([0, 0]);
        assert_eq!((m.sub, m.phase), (Sub::Repick, repick::INIT));
        // First frame: script 0x42, no VM; then 0x14 frames of slide-in.
        m.step([0, 0]);
        assert_eq!((m.scripts.as_slice(), m.run_sd, m.bombs), (&[script::REPICK][..], false, [0, 0]));
        for _ in 0..0x14 {
            m.step([0, 0]);
            assert!(m.run_sd);
        }
        assert_eq!(m.phase, repick::PICK);
        // Player 1: down to 4, two bombs, confirm; the arrow plays 0x1A frames.
        m.step([pad::DOWN, pad::UP]);
        m.step([pad::RIGHT, 0]);
        m.step([0, 0]);
        m.step([pad::RIGHT, 0]);
        m.step([pad::CROSS, 0]);
        assert_eq!((m.repick[0].state, m.repick[0].arrow_frame), (2, 1));
        assert_eq!(m.repick_palettes[3], 1);
        assert_eq!((m.difficulty, m.bombs), ([4, 2], [2, 0]));
        assert_eq!(m.repick[1].arrow, (REPICK_ARROW_X[1], REPICK_ARROW_Y[1]));
        for _ in 0..0x19 {
            m.step([0, 0]);
        }
        assert_eq!(m.repick[0].state, 2);
        m.step([0, 0]);
        assert_eq!((m.repick[0].state, m.repick[0].arrow_frame), (3, 0));
        // △ takes it back and unlights the item.
        m.step([pad::TRIANGLE, 0]);
        assert_eq!((m.repick[0].state, m.repick_palettes[3]), (0, 0));
        m.step([pad::CROSS, pad::CROSS]);
        for _ in 0..0x1A {
            m.step([0, 0]);
        }
        assert_eq!(m.phase, repick::OUT);
        assert_eq!(m.scripts, [script::REPICK_DONE]);
        let at = m.repick[0].arrow;
        for _ in 0..0x14 {
            m.step([0, 0]);
        }
        assert_eq!(m.repick[0].arrow, at);
        m.step([0, 0]);
        assert_eq!(m.repick[0].arrow, (at.0 - 0x10, at.1 - 1));
        for _ in 0x15..0x4F {
            m.step([0, 0]);
        }
        assert_eq!(m.sub, Sub::Repick);
        m.step([0, 0]);
        assert_eq!(m.sub, Sub::Init);
        m.step([0, 0]);
        assert_eq!((m.sub, m.difficulty, m.wins), (Sub::Play, [4, 2], [0, 0]));
        assert_eq!(m.versus.games[0].bombs, 2);
    }

    fn course() -> Course {
        Course { menu_pad: 0, course: 1, stage: 6, last_stage: 14, scores: [5000, 70], all_clears: [0; 2], wiggle: 9, carry: Default::default() }
    }

    /// Player 1 (pad 0) loses two sets to the challenger; the match end runs to `outcome`.
    fn challenger_wins(m: &mut SetMatch) {
        for _ in 0..2 {
            m.start_set(7);
            m.versus.games[0].player.state = ps::GAME_OVER;
            m.end_check();
        }
        assert_eq!((m.sub, m.winner), (Sub::MatchEnd, Some(1)));
    }

    /// Modes 13 → 15 → 13: RAM the inits never write (delays, soft-drop points) carries over.
    #[test]
    fn the_delays_and_drop_points_survive_the_hand_overs() {
        use crate::game::Carry;
        let k = [Carry { flip_cd: 3, move_delay: 7, drop_points: 10 }, Carry { flip_cd: 1, move_delay: 2, drop_points: 4 }];
        let m = SetMatch::challenger(3, Course { carry: k, ..course() });
        assert_eq!(m.versus.games.each_ref().map(|g| g.carry()), k);
        let v = crate::vs_cpu::VsCpu::resume(5, Course { carry: k, ..course() });
        assert_eq!(v.course_state().carry, k);
    }

    #[test]
    fn a_challenger_match_returns_the_winner_to_the_course() {
        let mut m = SetMatch::challenger(3, course());
        assert_eq!((m.difficulty, m.bombs, m.sets), ([3, 3], [0, 0], 3));
        assert_eq!(m.versus.games[0].hud.score, 5000);
        challenger_wins(&mut m);
        let mut t = 0;
        while m.outcome.is_none() {
            m.step([0, 0]);
            t += 1;
            assert!(t < 3000);
        }
        // func_8002D114 phase 5 stops the music on its last frame.
        assert_eq!(m.music, Some(Request::Stop));
        let back = m.back.expect("back to the course");
        assert_eq!((back.menu_pad, back.stage, back.last_stage, back.wiggle), (1, 6, 14, 9));
        assert_eq!(back.scores, m.scores);
        assert_eq!(m.tally, [0, 1]);
        // Mode 14 just goes to the title.
        let mut m = SetMatch::new(3, [2, 2], [0, 0], 1);
        m.versus.games[1].player.state = ps::GAME_OVER;
        m.end_check();
        while m.outcome.is_none() {
            m.step([0, 0]);
        }
        assert_eq!(m.back, None);
    }

    #[test]
    fn a_challenger_rematch_skips_the_repick() {
        let mut m = SetMatch::challenger(3, course());
        challenger_wins(&mut m);
        while m.over.phase != crate::over::phase::COUNTDOWN {
            m.step([0, 0]);
        }
        // The loser (pad 0) continues: the menu pad (still pad 0) gets the point.
        m.step([pad::START, 0]);
        let mut stops = 0;
        for _ in 0..crate::over::RETRY_FRAMES + 1 {
            m.step([0, 0]);
            stops += usize::from(m.music == Some(Request::Stop));
        }
        assert_eq!(stops, 1);
        assert_eq!((m.sub, m.wins, m.menu_pad), (Sub::Play, [0, 0], 1));
        assert_eq!(m.course.map(|c| c.menu_pad), Some(1));
        assert_eq!(m.scores[0], m.versus.games[0].hud.score);
        assert_eq!(m.tally, [0, 1]);
    }

    #[test]
    fn a_port_change_pauses_with_that_pads_menu() {
        let mut m = SetMatch::new(3, [2, 2], [0, 0], 1);
        m.port_change = [false, true];
        m.step([0; 2]);
        assert_eq!(m.sub, Sub::Play, "not before the clock runs");
        while !m.versus.games[0].clock_running() {
            m.step([0; 2]);
        }
        // Pad 1's Start, then port 2's change on the same frame: port 2 owns the menu.
        m.port_change = [false, true];
        m.step([pad::START, 0]);
        assert_eq!((m.sub, m.paused_by), (Sub::Paused, 1));
        m.step([0, pad::START]);
        assert_eq!(m.sub, Sub::Play);
        // Both ports at once: port 1 is checked first.
        m.port_change = [true, true];
        m.step([0; 2]);
        assert_eq!((m.sub, m.paused_by), (Sub::Paused, 0));
    }
}
