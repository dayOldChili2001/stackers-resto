//! Two players on one RNG, with garbage between them (the two-player frame of modes 5, 11, 13…).
//!
//! Frame order (`func_8001E07C`, `func_800202AC`, `func_8003A82C`): both player updates, then
//! both cell updates (each starting with its garbage step), then the score step, which sends each
//! player's attack to the other's pool. The BIOS `rand()` and the piece ring are global in the
//! original, so one copy lives here and is swapped into whichever player is running.

use crate::cpu::Cpu;
use crate::game::{pad, ps, Game, PieceRing};
use crate::rng::BiosRand;

#[derive(Clone, Debug)]
pub struct Versus {
    pub games: [Game; 2],
    pub(crate) rng: BiosRand,
    ring: PieceRing,
    prev_held: [u16; 2],
    pub frame: u64,
    /// Computer players (`func_80034C60`). A CPU's buttons replace its pad's, except Start and
    /// Select (`func_80014F20` keeps `0x900` of the real pad).
    pub cpu: [Option<Cpu>; 2],
}

impl Versus {
    /// `func_8001FCA0` for player 0, then player 1. With equal colour counts player 1 starts with
    /// player 0's three pairs and no `rand()` calls (`func_80023474`), and later pairs come from
    /// the shared ring; otherwise player 1 rolls its own queue next.
    pub fn new(seed: u32, levels: [usize; 2]) -> Self {
        let g0 = Game::new(seed, levels[0]);
        let g1 = Game::new(g0.rng.seed, levels[1]);
        Self::pair(g0, g1)
    }

    /// Mode 14's players (`func_80039DE8`, then `func_8001FC64`): difficulty 1–5 and bombs
    /// per player.
    pub fn set_match(seed: u32, difficulty: [u8; 2], bombs: [u8; 2]) -> Self {
        let g0 = Game::set_match(seed, difficulty[0], bombs[0]);
        let g1 = Game::set_match(g0.rng.seed, difficulty[1], bombs[1]);
        Self::pair(g0, g1)
    }

    fn pair(g0: Game, mut g1: Game) -> Self {
        let shared = g0.level_info().colors == g1.level_info().colors;
        let rng = if shared {
            g1.queue = g0.queue;
            g0.rng.clone()
        } else {
            g1.rng.clone()
        };
        let mut games = [g0, g1];
        for g in &mut games {
            g.shared_ring = shared;
        }
        Versus { games, rng, ring: PieceRing::default(), prev_held: [0; 2], frame: 0, cpu: [None, None] }
    }

    /// Mode 11's init (`func_80016BB4` sub-state 0): both players on level row 3 with the
    /// Time Trial rules, player 1 a garbage [`Game::target`], and `layout` (a course preset,
    /// `func_80027098(1, course)`) falling into player 0's field.
    pub fn time_trial(seed: u32, layout: &[i16]) -> Self {
        let mut v = Versus::new(seed, [crate::tables::SOLO_ROW; 2]);
        for g in &mut v.games {
            g.rules = crate::game::Rules::TimeTrial;
            g.reset_intro();
        }
        v.games[1].target = true;
        v.games[0].field.load_preset(layout);
        v
    }

    /// The BIOS `rand()` state, for a retry that keeps the sequence going.
    pub fn seed(&self) -> u32 {
        self.rng.seed
    }

    /// Run `f` on player `i` with the shared RNG and ring swapped in.
    fn with(&mut self, i: usize, f: impl FnOnce(&mut Game)) {
        let g = &mut self.games[i];
        std::mem::swap(&mut self.rng, &mut g.rng);
        std::mem::swap(&mut self.ring, &mut g.ring);
        f(g);
        std::mem::swap(&mut self.rng, &mut g.rng);
        std::mem::swap(&mut self.ring, &mut g.ring);
    }

    /// The player who topped out first (player 0 if both did on the same frame).
    pub fn loser(&self) -> Option<usize> {
        self.games.iter().position(|g| g.is_over())
    }

    /// Advance one frame with each pad's held buttons. Does nothing once a player has lost.
    pub fn step(&mut self, mut held: [u16; 2]) {
        if self.loser().is_some() {
            return;
        }
        // The pad read runs the AI first (player 0's, then player 1's) on last frame's state.
        for i in 0..2 {
            if let Some(cpu) = &mut self.cpu[i] {
                held[i] = (held[i] & (pad::START | pad::SELECT)) | cpu.step(&self.games[i], &mut self.rng);
            }
        }
        let pressed = [held[0] & !self.prev_held[0], held[1] & !self.prev_held[1]];
        for i in 0..2 {
            self.with(i, |g| g.play_player(held[i], pressed[i]));
        }
        for i in 0..2 {
            let opponent = self.games[1 - i].player.state;
            self.games[i].opponent_state = opponent;
            self.with(i, |g| g.play_cells(held[i], pressed[i]));
        }
        // func_8003A82C: each player's blobs go to the other's pool.
        for i in 0..2 {
            let sent = std::mem::take(&mut self.games[i].outgoing);
            let pool = &mut self.games[1 - i].garbage.incoming[0];
            *pool = pool.wrapping_add(sent as i16);
        }
        // func_80038A08 (after the HUD script): the chain and bomb cut-ins.
        for g in &mut self.games {
            g.step_cut_in();
        }
        // func_8003AA78: the per-turn sent count resets while the player is in state 4.
        for g in &mut self.games {
            if g.player.state == ps::NO_MATCH {
                g.garbage.sent_this_turn = 0;
            }
        }
        for i in 0..2 {
            self.games[i].prev_held = held[i];
            self.games[i].frame += 1;
        }
        self.prev_held = held;
        self.frame += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_get_the_same_pieces() {
        let mut v = Versus::new(7, [3, 3]);
        assert_eq!(v.games[0].queue, v.games[1].queue);
        let mut seen = [Vec::new(), Vec::new()];
        // Player 0 runs ahead, player 1 catches up and overtakes.
        for i in [0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 1, 0, 0, 0] {
            let mut p = (0, 0);
            v.with(i, |g| p = g.next_pair());
            seen[i].push(p);
        }
        assert_eq!(seen[0][..7], seen[1][..]);
        assert!(seen[0].windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn different_colour_counts_roll_separately() {
        let v = Versus::new(7, [0, 13]);
        assert!(!v.games[0].shared_ring);
        assert_ne!(v.games[0].queue, v.games[1].queue);
    }

    #[test]
    fn a_chain_sends_garbage_that_drops_after_the_turn() {
        let mut v = Versus::new(7, [3, 3]);
        v.games[0].outgoing = 9;
        v.step([0, 0]);
        let g = &v.games[1].garbage;
        assert_eq!(g.incoming[0], 9);
        v.step([0, 0]);
        let g = &v.games[1].garbage;
        assert_eq!(g.incoming[0], 0);
        assert_eq!(g.incoming[1..].iter().sum::<i16>(), 9);
    }

    #[test]
    fn pending_garbage_drops_grey_when_the_piece_locks() {
        use crate::field::st;
        let mut v = Versus::new(7, [3, 3]);
        v.games[1].garbage.incoming[0] = 12;
        let mut dropped = 0;
        for _ in 0..600 {
            v.step([0, crate::game::pad::DOWN]);
            dropped += v.games[1].garbage_dropped;
            if dropped > 0 {
                break;
            }
        }
        assert_eq!(dropped, 12);
        let f = &v.games[1].field;
        let grey = (1..7)
            .flat_map(|c| (0..18).map(move |r| (c, r)))
            .filter(|&(c, r)| f.cell(c, r).color == 0 && f.cell(c, r).state != st::EMPTY)
            .count();
        assert_eq!(grey, 12);
        assert_eq!(v.games[0].garbage.dropped_total, 0);
    }
}
