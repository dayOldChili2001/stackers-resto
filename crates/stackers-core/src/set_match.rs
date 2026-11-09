//! Arcade's two-player set match (mode 14, `func_80015FCC`).
//!
//! Sub-states (`D_800C34AA`, `jtbl_80010118`): 0 init, 1 play, 2 set result
//! (`func_8003D184`), 3 pause (`func_8002E59C`), 4 match end (`func_8002D114`), 12 the in-game
//! difficulty re-pick (`func_8002B8EC`). The first to win `sets / 2 + 1` sets wins the match.
//! Each player has a difficulty (`D_80102740`, row of `D_800120D4`) and a bomb stock
//! (`D_800FFCF8`, copied to `D_80100674` at the start of every set).

use crate::game::{pad, ps};
use crate::over::{GameOver, Outcome};
use crate::pause::Pause;
use crate::roulette::{ColumnTally, Roulette};
use crate::versus::Versus;

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
    Play,
    Paused,
    SetResult,
    MatchEnd,
    Repick,
}

/// One player's column of the re-pick screen (`D_800B1400[p]`, `D_800B1340[p]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Repick {
    /// 0–4: difficulty − 1.
    pub cursor: u8,
    /// 0 choosing, 2 just confirmed (blinks for 0x1B frames), 3 ready (△ goes back).
    pub state: u8,
    pub timer: u16,
}

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
    /// Set when the run leaves the mode (Quit, or nobody continued).
    pub outcome: Option<Outcome>,
    pub sounds: Vec<u8>,
    pub scripts: Vec<u8>,
    prev: [u16; 2],
    pub frame: u64,
}

impl SetMatch {
    pub fn new(seed: u32, difficulty: [u8; 2], bombs: [u8; 2], sets: u8) -> SetMatch {
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
            repick: [Repick::default(); 2],
            outcome: None,
            sounds: Vec::new(),
            scripts: Vec::new(),
            prev: [0; 2],
            frame: 0,
        };
        m.start_set(seed);
        m
    }

    /// Sub-state 0: fresh fields, levels and stock; the HUD indicators back to column 2.
    fn start_set(&mut self, seed: u32) {
        let bombs = self.bombs.map(|b| b.min(MAX_BOMBS));
        self.versus = Versus::set_match(seed, self.difficulty, bombs);
        self.roulette = [Roulette::new(); 2];
        self.tallies = [0, 1].map(|p| {
            let g = &self.versus.games[p].garbage;
            std::array::from_fn(|c| ColumnTally::new(g.incoming[c + 1] + g.pending[c + 1]))
        });
        self.sync_columns();
        self.sub = Sub::Play;
        self.winner = None;
        self.pause = Pause::default();
        self.over = GameOver::default();
        self.phase = 0;
        self.timer = 0;
        self.scripts.push(script::READY);
    }

    /// The pad that answers the result screens: the loser's (`(D_800B1238 + 1) & 1`).
    fn loser_pad(&self) -> usize {
        1 - self.winner.unwrap_or(0)
    }

    fn sync_columns(&mut self) {
        for (g, r) in self.versus.games.iter_mut().zip(&self.roulette) {
            g.special_column = Some(usize::from(r.column) + 1);
        }
    }

    /// One 60 Hz frame with both pads' held buttons.
    pub fn step(&mut self, held: [u16; 2]) {
        let pressed = [held[0] & !self.prev[0], held[1] & !self.prev[1]];
        self.prev = held;
        self.sounds.clear();
        self.scripts.clear();
        self.frame += 1;
        if self.outcome.is_some() {
            return;
        }
        match self.sub {
            Sub::Play => self.play(held, pressed),
            Sub::Paused => {
                let mut sounds = Vec::new();
                self.pause.step(pressed[self.paused_by], |id| sounds.push(id));
                self.sounds = sounds;
                self.step_spans();
                match self.pause.outcome {
                    Some(o) => self.outcome = Some(o),
                    None if !self.pause.active => self.sub = Sub::Play,
                    None => {}
                }
            }
            Sub::SetResult => self.set_result(pressed),
            Sub::MatchEnd => self.match_end(pressed),
            Sub::Repick => self.step_repick(pressed),
        }
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
        // func_8003BCD0: the special-column indicators.
        for p in 0..2 {
            let g = &self.versus.games[p];
            let r = &mut self.roulette[p];
            // func_8003C340: the chain value going 1 → 2 starts the spin.
            if g.stats.chain == 2 && g.chain_before == 1 {
                r.start(1);
            }
            // func_8003C3D8: the turn ending, ○ or ✕ stop it.
            if r.script == 1
                && (g.player.state == ps::NO_MATCH || pressed[p] & (pad::CIRCLE | pad::CROSS) != 0)
            {
                r.start(2);
            }
            if r.step() && !sounds.contains(&crate::game::se::ROULETTE) {
                sounds.push(crate::game::se::ROULETTE);
            }
            // func_8003C5A0 + func_8003C1AC: the incoming-garbage digits.
            let gb = &g.garbage;
            for (c, t) in self.tallies[p].iter_mut().enumerate() {
                t.step(gb.incoming[c + 1] + gb.pending[c + 1]);
            }
        }
        self.sync_columns();
        self.sounds = sounds;
        let running = self.versus.games[0].clock_running();
        if running {
            if let Some(p) = (0..2).find(|&p| pressed[p] == pad::START) {
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
        let over = [0, 1].map(|p| self.versus.games[p].player.state == ps::GAME_OVER);
        let winner = match over {
            [false, false] => return,
            [true, false] => 1,
            _ => 0,
        };
        self.winner = Some(winner);
        self.wins[winner] += 1;
        for r in &mut self.roulette {
            if r.script == 1 {
                r.start(2);
            }
        }
        for g in &mut self.versus.games {
            g.stop_clock();
        }
        self.sub = Sub::SetResult;
        self.phase = 0;
        self.timer = 0;
        // func_8003B2B8, modes 13–15: the loser bursts, the winner's blobs pop.
        let loser = 1 - winner;
        let v = &mut self.versus;
        let (games, rng) = (&mut v.games, &mut v.rng);
        games[loser].field.burst(|| rng.next());
        games[loser].player.state = ps::GAME_OVER;
        games[winner].field.pop_all();
        games[winner].player.state = ps::GAME_OVER;
        self.sounds.push(0x1E);
        if self.wins[winner] >= self.sets / 2 + 1 {
            self.tally[winner] = (self.tally[winner] + 1).min(999);
            // func_8002D0C4: straight to sub-state 4.
            self.sub = Sub::MatchEnd;
            self.over.begin(false);
        }
    }

    /// `func_8003B03C`, modes 13–15.
    fn step_end_fields(&mut self) {
        let Some(w) = self.winner else { return };
        self.versus.games[1 - w].field.burst_step();
        self.versus.games[w].field.pop_special_step();
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
                    let seed = self.versus.seed();
                    self.start_set(seed);
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
        if first {
            // func_8002D91C, mode 14.
            self.scripts = vec![if self.winner == Some(1) { script::MATCH_P2 } else { script::MATCH_P1 }];
            self.over.message = self.scripts[0];
        }
        if moving {
            self.step_end_fields();
        }
        self.step_spans();
        match self.over.outcome {
            // Phase 4 in mode 14: wins reset, then the re-pick.
            Some(Outcome::Retry) => {
                self.wins = [0; 2];
                self.sub = Sub::Repick;
                self.phase = 0;
                self.timer = 0;
                self.repick = [Repick { cursor: 2, ..Default::default() }; 2];
                self.bombs = [0; 2];
                self.scripts.push(script::REPICK);
            }
            Some(Outcome::Title) => self.outcome = Some(Outcome::Title),
            None => {}
        }
    }

    /// Sub-state 12 (`func_8002B8EC`): both players pick a difficulty (↑/↓) and bombs (←/→),
    /// ✕ or Start to confirm, △ to take it back; once both are ready the next match starts.
    fn step_repick(&mut self, pressed: [u16; 2]) {
        self.timer += 1;
        match self.phase {
            // Phase 1: the lists slide in for 0x14 frames.
            0 => {
                if self.timer >= 0x14 {
                    self.phase = 1;
                    self.timer = 0;
                }
            }
            1 => {
                for p in 0..2 {
                    let r = &mut self.repick[p];
                    let b = pressed[p];
                    if r.state == 3 && b == pad::TRIANGLE {
                        r.state = 0;
                        continue;
                    }
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
                            r.state = 2;
                            r.timer = 0;
                            self.difficulty[p] = r.cursor + 1;
                            self.sounds.push(0xA8);
                        }
                    } else if r.state == 2 {
                        r.timer += 1;
                        if r.timer >= 0x1B {
                            r.state = 3;
                        }
                    }
                }
                if self.repick.iter().all(|r| r.state == 3) {
                    self.phase = 2;
                    self.timer = 0;
                    self.scripts.push(script::REPICK_DONE);
                }
            }
            // The lists fly out; after 0x50 frames the match starts over.
            _ => {
                if self.timer >= 0x50 {
                    let seed = self.versus.seed();
                    self.start_set(seed);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Rules;

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
        m.sync_columns();
        assert_eq!(m.versus.games[0].special_column, Some(4));
    }
}
