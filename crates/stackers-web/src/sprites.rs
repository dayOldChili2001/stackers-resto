//! Real blob sprites: runs the `BLT_ANM.OBJ` scripts alongside the game and returns draw lists.
//!
//! The core only tracks how long each animation has run (`Anim::age`, `Anim::id`); this mirrors
//! the original's script player per cell so the drawn frame matches `func_80021D44`.

use crate::session::Session;
use stackers_core::field::{st, WALL};
use stackers_core::{Cell, COLS, ROWS};
use stackers_formats::anm::{AnimBank, CellKey, Frame, ScriptPlayer};
use stackers_formats::tim::Tim;
use wasm_bindgen::prelude::*;

/// Ints per sprite in a draw list: `[page, clut, u, v, w, h, x, y, flip]`.
pub const SPRITE_STRIDE: usize = 9;
/// Sprite sheets `DERODERO/GRAPH/DERO/DERO{page}.TIM`.
pub const PAGES: u32 = 10;
/// Steps to fast-forward at most when a script has to be rebuilt from its age.
const MAX_REPLAY: u16 = 4096;

#[derive(Clone, Copy, Default)]
struct Mirror {
    id: u32,
    key: CellKey,
    age: u16,
    player: ScriptPlayer,
    live: bool,
}

impl Mirror {
    /// Bring the script player in line with an animation that has run `age` frames.
    fn sync(&mut self, bank: &AnimBank, id: u32, key: CellKey, age: u16, script: Option<u32>) {
        let Some(script) = script else {
            self.live = false;
            return;
        };
        let fresh = !self.live || self.id != id || age < self.age;
        if fresh {
            self.player = ScriptPlayer::new(script);
            for _ in 0..age.min(MAX_REPLAY) {
                self.player.step(bank);
            }
        } else {
            if key != self.key {
                self.player.restart_keep_elapsed(bank, script);
            }
            for _ in 0..(age - self.age).min(MAX_REPLAY) {
                self.player.step(bank);
            }
        }
        *self = Mirror { id, key, age, live: true, ..*self };
    }
}

#[wasm_bindgen]
pub struct Sprites {
    bank: AnimBank,
    cells: Vec<Mirror>,
    piece: [Mirror; 2],
}

fn emit(out: &mut Vec<i32>, bank: &AnimBank, frame_ptr: u32, x: i32, y: i32) {
    let Some(f) = bank.frame(frame_ptr) else { return };
    if f.w <= 0 || f.h <= 0 || !(0..PAGES as i16).contains(&f.page) {
        return;
    }
    out.extend_from_slice(&[
        i32::from(f.page),
        i32::from(f.clut),
        i32::from(f.u),
        i32::from(f.v),
        i32::from(f.w),
        i32::from(f.h),
        x + i32::from(f.dx),
        y + i32::from(f.dy),
        i32::from(f.flip != 0),
    ]);
}

#[wasm_bindgen]
impl Sprites {
    /// `blt_anm` is `DERODERO/DATA/BLT_ANM.OBJ` from the user's disc.
    #[wasm_bindgen(constructor)]
    pub fn new(blt_anm: &[u8]) -> Result<Sprites, JsError> {
        let bank = AnimBank::parse(blt_anm).map_err(crate::js_err)?;
        Ok(Sprites { bank, cells: vec![Mirror::default(); COLS * ROWS], piece: [Mirror::default(); 2] })
    }

    /// Forget all script state (call after starting a new session).
    pub fn reset(&mut self) {
        self.cells.fill(Mirror::default());
        self.piece = [Mirror::default(); 2];
    }

    /// Sprites for the field and the controlled pair, in draw order, in field pixels.
    pub fn field(&mut self, session: &Session) -> Vec<i32> {
        let game = session.game();
        let mut out = Vec::with_capacity(128 * SPRITE_STRIDE);
        for r in 0..ROWS - 1 {
            for c in 1..COLS - 1 {
                let cell: &Cell = game.field.cell(c, r);
                let m = &mut self.cells[c * ROWS + r];
                if cell.state == st::EMPTY || cell.color == 0 || cell.color == WALL {
                    m.live = false;
                    continue;
                }
                let key = CellKey { color: cell.color, state: cell.state, mask: cell.mask, arms: cell.arms };
                m.sync(&self.bank, cell.anim.id, key, cell.anim.age, self.bank.cell_script(key));
                if m.live {
                    emit(&mut out, &self.bank, m.player.frame, cell.x, cell.y);
                }
            }
        }
        if game.piece_active() {
            let pa = game.piece_anim;
            let p = &game.player;
            let (sx, sy) = game.satellite_xy();
            for (i, (color, x, y)) in [(p.pivot, p.x, p.y), (p.satellite, sx, sy)].into_iter().enumerate() {
                let key = CellKey { color, state: pa.state, ..Default::default() };
                let m = &mut self.piece[i];
                m.sync(&self.bank, pa.id, key, pa.age, self.bank.piece_script(color, pa.state));
                if m.live {
                    emit(&mut out, &self.bank, m.player.frame, x, y);
                }
            }
        } else {
            self.piece = [Mirror::default(); 2];
        }
        out
    }

    /// The still frame used for a queued pair half, drawn at (`x`, `y`).
    pub fn still(&self, color: u8, x: i32, y: i32) -> Vec<i32> {
        let mut out = Vec::new();
        if let Some(f) = self.still_ptr(color) {
            emit(&mut out, &self.bank, f, x, y);
        }
        out
    }
}

impl Sprites {
    fn still_ptr(&self, color: u8) -> Option<u32> {
        let s = self.bank.piece_script(color, 0)?;
        let mut p = ScriptPlayer::new(s);
        p.step(&self.bank);
        Some(p.frame)
    }

    /// First frame of the pair-half script for `color` (what the queue window shows).
    pub(crate) fn still_frame(&self, color: u8) -> Option<Frame> {
        let f = self.bank.frame(self.still_ptr(color)?)?;
        (f.w > 0 && f.h > 0 && (0..PAGES as i16).contains(&f.page)).then_some(f)
    }
}

/// Decode a TIM (e.g. a `DERO` sheet) with palette row `clut` to RGBA8.
#[wasm_bindgen]
pub fn decode_tim(data: &[u8], clut: u32) -> Result<crate::Image, JsError> {
    let tim = Tim::parse(data).map_err(crate::js_err)?;
    Ok(crate::Image::new(&tim, clut))
}

/// As [`decode_tim`], for a semi-transparent primitive: STP texels get alpha 128.
#[wasm_bindgen]
pub fn decode_tim_semi(data: &[u8], clut: u32, semi: bool) -> Result<crate::Image, JsError> {
    let tim = Tim::parse(data).map_err(crate::js_err)?;
    Ok(crate::Image::semi(&tim, clut, semi))
}
