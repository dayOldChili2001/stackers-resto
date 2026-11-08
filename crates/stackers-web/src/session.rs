//! A single-player game exposed to JavaScript. The page owns timing (60 Hz fixed step) and drawing.

use stackers_core::{game::DEATH_CELL, Game, COLS, ROWS};
use wasm_bindgen::prelude::*;

/// Ints per cell in [`Session::cells`].
pub const CELL_STRIDE: usize = 7;

#[wasm_bindgen]
pub struct Session {
    game: Game,
}

impl Session {
    pub(crate) fn game(&self) -> &Game {
        &self.game
    }
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32, level: u32) -> Session {
        Session { game: Game::new(seed, level as usize) }
    }

    /// Advance one frame. `buttons` uses the PS1 pad bits from `stackers_core::pad`.
    pub fn step(&mut self, buttons: u16) {
        self.game.step(buttons);
    }

    /// Playable cells (columns 1–6, rows 0–17), row-major, each
    /// `[col, row, state, colour, x_px, y_px, arms | anim_age << 8]`.
    pub fn cells(&self) -> Vec<i32> {
        let mut out = Vec::with_capacity((COLS - 2) * (ROWS - 1) * CELL_STRIDE);
        for r in 0..ROWS - 1 {
            for c in 1..COLS - 1 {
                let cell = self.game.field.cell(c, r);
                out.extend_from_slice(&[
                    c as i32,
                    r as i32,
                    i32::from(cell.state),
                    i32::from(cell.color),
                    cell.x,
                    cell.y,
                    i32::from(cell.arms) | i32::from(cell.anim.age) << 8,
                ]);
            }
        }
        out
    }

    /// `[active, x, y, pivot, sat_x, sat_y, satellite]` for the controlled pair.
    pub fn piece(&self) -> Vec<i32> {
        let p = &self.game.player;
        let (sx, sy) = self.game.satellite_xy();
        vec![
            i32::from(self.game.piece_active()),
            p.x,
            p.y,
            i32::from(p.pivot),
            sx,
            sy,
            i32::from(p.satellite),
        ]
    }

    /// Upcoming pairs after the current one: `[pivot, satellite, pivot, satellite]`.
    pub fn next(&self) -> Vec<u8> {
        let q = &self.game.queue;
        vec![q[0].0, q[0].1, q[1].0, q[1].1]
    }

    /// All three queued pairs, front first: `[pivot, satellite] × 3`.
    pub fn queue(&self) -> Vec<u8> {
        self.game.queue.iter().flat_map(|&(p, s)| [p, s]).collect()
    }

    /// `[level, colours, cleared, chain, max_chain, attack, pieces, player_state, game_over,
    /// speed_index]`.
    pub fn stats(&self) -> Vec<u32> {
        let s = &self.game.stats;
        let lv = self.game.level_info();
        vec![
            self.game.level as u32,
            u32::from(lv.colors),
            s.cleared,
            u32::from(s.chain),
            u32::from(s.max_chain),
            s.attack,
            s.pieces,
            u32::from(self.game.player.state),
            u32::from(self.game.is_over()),
            u32::from(self.game.speed_index),
        ]
    }

    /// Sound-effect ids triggered by the last [`Session::step`] (see `stackers_core::se`).
    pub fn sounds(&self) -> Vec<u8> {
        self.game.sounds.clone()
    }

    pub fn frame(&self) -> u32 {
        self.game.frame as u32
    }

    #[wasm_bindgen(getter)]
    pub fn death_row(&self) -> u32 {
        DEATH_CELL.1 as u32
    }
}
