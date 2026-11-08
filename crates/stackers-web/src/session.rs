//! A single-player game exposed to JavaScript. The page owns timing (60 Hz fixed step) and drawing.

use stackers_core::{game::DEATH_CELL, Game, Outcome, COLS, ROWS};
use stackers_formats::sdchr::{SdBank, SdStage};
use wasm_bindgen::prelude::*;

/// Ints per cell in [`Session::cells`].
pub const CELL_STRIDE: usize = 7;

#[wasm_bindgen]
pub struct Session {
    game: Game,
    sd: Option<SdBank>,
    stage: SdStage,
    sounds: Vec<u8>,
}

impl Session {
    pub(crate) fn game(&self) -> &Game {
        &self.game
    }

    pub(crate) fn stage(&self) -> &SdStage {
        &self.stage
    }

    fn with_game(game: Game) -> Session {
        Session { game, sd: None, stage: SdStage::default(), sounds: Vec::new() }
    }
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32, level: u32) -> Session {
        Session::with_game(Game::new(seed, level as usize))
    }

    /// A solo-mode game (mode 4 rules).
    pub fn solo(seed: u32) -> Session {
        Session::with_game(Game::solo(seed))
    }

    /// Advance one frame. `buttons` uses the PS1 pad bits from `stackers_core::pad`.
    pub fn step(&mut self, buttons: u16) {
        self.game.step(buttons);
        self.sounds.clone_from(&self.game.sounds);
        // func_8002EB38 after the game-over handler: start any requested script, then run
        // every object one frame.
        if let Some(bank) = &self.sd {
            for &id in &self.game.over.scripts {
                self.stage.start(bank, id);
            }
            if self.game.over.active && self.stage.active {
                for id in self.stage.step(bank) {
                    if !self.sounds.contains(&id) {
                        self.sounds.push(id);
                    }
                }
            }
        }
    }

    /// `DATA/SD_CHR.OBJ` from the user's disc: the game-over messages.
    pub fn set_sd_chr(&mut self, data: &[u8]) -> Result<(), JsError> {
        self.sd = Some(SdBank::parse(data).map_err(crate::js_err)?);
        Ok(())
    }

    /// The best erase number so far; game over compares against it ("new record" emblem).
    #[wasm_bindgen(setter)]
    pub fn set_record(&mut self, record: u32) {
        self.game.record = record;
    }

    #[wasm_bindgen(getter)]
    pub fn record(&self) -> u32 {
        self.game.record
    }

    /// How the game-over sequence ended: 0 still running (or not over), 1 retry (Start during
    /// the countdown), 2 back to the title (the countdown ran out).
    #[wasm_bindgen(getter)]
    pub fn outcome(&self) -> u32 {
        match self.game.over.outcome {
            None => 0,
            Some(Outcome::Retry) => 1,
            Some(Outcome::Title) => 2,
        }
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

    /// Slot y of the three queued pairs in the next-pair window (they slide up after a spawn).
    pub fn queue_y(&self) -> Vec<i32> {
        self.game.queue_y.to_vec()
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
            self.game.speed_index as u32,
        ]
    }

    /// `[score, erase number, course time]`; the time is packed one byte each (low to high):
    /// frames, seconds, minutes, hours.
    pub fn hud(&self) -> Vec<u32> {
        let h = self.game.hud;
        vec![h.score, h.erased, h.time]
    }

    /// Sound-effect ids triggered by the last [`Session::step`] (see `stackers_core::se`).
    pub fn sounds(&self) -> Vec<u8> {
        self.sounds.clone()
    }

    pub fn frame(&self) -> u32 {
        self.game.frame as u32
    }

    #[wasm_bindgen(getter)]
    pub fn death_row(&self) -> u32 {
        DEATH_CELL.1 as u32
    }
}
