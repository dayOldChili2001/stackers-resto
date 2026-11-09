//! Real blob sprites: runs the `BLT_ANM.OBJ` scripts alongside the game and returns draw lists.
//!
//! The core only tracks how long each animation has run (`Anim::age`, `Anim::id`); this mirrors
//! the original's script player per cell so the drawn frame matches `func_80021D44`.

use crate::session::Session;
use stackers_core::field::{st, Span, LEFT, RIGHT, WALL};
use stackers_core::tables::span_index;
use stackers_core::{Cell, Game, COLS, ROWS, TILE};
use stackers_formats::anm::{AnimBank, CellKey, Frame, ScriptPlayer, Segment};
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

/// The script players behind one arm span (`D_800C3538`): the arm itself and its joints.
#[derive(Clone, Default)]
struct SpanMirror {
    id: u32,
    steps: u16,
    seg_steps: u16,
    player: ScriptPlayer,
    /// Column offset along the arm (signed toward the arm's side) and the player.
    segs: Vec<(i32, ScriptPlayer)>,
}

impl SpanMirror {
    /// `func_80025234` / `func_80025338`: the scripts a span starts with.
    fn start(bank: &AnimBank, span: &Span, side: usize) -> SpanMirror {
        let script = bank.span_script(side, span_index(span.mode, span.end, span.len)).unwrap_or(0);
        let mut segs = Vec::new();
        if span.mode != -1 {
            let dir = if side == RIGHT { 1 } else { -1 };
            let cleared = span.mode == 0;
            let mut add = |seg: Segment, k: u8| {
                if let Some(s) = bank.segment_script(seg, side, span.end, span.len, cleared) {
                    segs.push((dir * i32::from(k), ScriptPlayer::span(s)));
                }
            };
            if span.target != 0 {
                add(Segment::End, span.target);
            }
            for k in 1..6u8 {
                if span.below >> (k - 1) & 1 != 0 {
                    add(Segment::Below(k), k);
                }
            }
            for k in 1..6u8 {
                if span.above >> (k - 1) & 1 != 0 {
                    add(Segment::Above(k), k);
                }
            }
        }
        SpanMirror { id: span.id, player: ScriptPlayer::span(script), segs, ..Default::default() }
    }

    fn sync(&mut self, bank: &AnimBank, span: &Span, side: usize) {
        if self.id != span.id || span.steps < self.steps || span.seg_steps < self.seg_steps {
            *self = SpanMirror::start(bank, span, side);
        }
        for _ in self.steps..span.steps.min(MAX_REPLAY) {
            self.player.step(bank);
        }
        for _ in self.seg_steps..span.seg_steps.min(MAX_REPLAY) {
            for (_, p) in self.segs.iter_mut() {
                p.step(bank);
            }
        }
        self.steps = span.steps;
        self.seg_steps = span.seg_steps;
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Sprites {
    bank: AnimBank,
    cells: Vec<Mirror>,
    /// `[(column · rows + row) · 2 + side]`.
    spans: Vec<SpanMirror>,
    piece: [Mirror; 2],
    /// Next-pair window, `[slot][satellite, pivot]`.
    queue: [[Mirror; 2]; 3],
}

#[wasm_bindgen]
impl Sprites {
    /// `blt_anm` is `DERODERO/DATA/BLT_ANM.OBJ` from the user's disc.
    #[wasm_bindgen(constructor)]
    pub fn new(blt_anm: &[u8]) -> Result<Sprites, JsError> {
        let bank = AnimBank::parse(blt_anm).map_err(crate::js_err)?;
        Ok(Sprites {
            bank,
            cells: vec![Mirror::default(); COLS * ROWS],
            spans: vec![SpanMirror::default(); COLS * ROWS * 2],
            piece: [Mirror::default(); 2],
            queue: [[Mirror::default(); 2]; 3],
        })
    }

    /// Forget all script state (call after starting a new session).
    pub fn reset(&mut self) {
        self.cells.fill(Mirror::default());
        self.spans.fill(SpanMirror::default());
        self.piece = [Mirror::default(); 2];
        self.queue = [[Mirror::default(); 2]; 3];
    }

    /// Sprites for the field and the controlled pair, in draw order, in field pixels.
    pub fn field(&mut self, session: &Session) -> Vec<i32> {
        let mut out = Vec::with_capacity(128 * SPRITE_STRIDE);
        for (f, x, y, part) in self.field_frames(session.game()) {
            let clut = match part {
                Part::Span { clut } => clut,
                _ => f.clut,
            };
            out.extend_from_slice(&[
                i32::from(f.page),
                i32::from(clut),
                i32::from(f.u),
                i32::from(f.v),
                i32::from(f.w),
                i32::from(f.h),
                x + i32::from(f.dx),
                y + i32::from(f.dy),
                i32::from(f.flip != 0),
            ]);
        }
        out
    }
}

/// What a [`Sprites::field_frames`] entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Cell,
    Pivot,
    Satellite,
    /// An arm span or one of its joints (`func_800258A8`, OT layer 1), drawn with the
    /// reaching blob's palette row.
    Span { clut: i16 },
}

impl Sprites {
    /// The field's blobs (rows top down, columns 1–6), then the pair's pivot and satellite at
    /// [`stackers_core::game::Game::pair_draw`]: each frame with its anchor in field pixels
    /// (the frame's own `dx`, `dy` still to be added, scaled).
    pub(crate) fn field_frames(&mut self, game: &Game) -> Vec<(Frame, i32, i32, Part)> {
        let rows = game.field.rows();
        if self.cells.len() != COLS * rows {
            self.cells = vec![Mirror::default(); COLS * rows];
        }
        let mut out = Vec::with_capacity(128);
        for r in 0..rows - 1 {
            for c in 1..COLS - 1 {
                let cell: &Cell = game.field.cell(c, r);
                let m = &mut self.cells[c * rows + r];
                if cell.state == st::EMPTY || cell.color == WALL {
                    m.live = false;
                    continue;
                }
                let key = CellKey { color: cell.color, state: cell.state, mask: cell.mask, arms: cell.arms };
                m.sync(&self.bank, cell.anim.id, key, cell.anim.age, self.bank.cell_script(key));
                let (live, frame) = (m.live, m.player.frame);
                if let Some(f) = self.drawable(frame).filter(|_| live) {
                    out.push((f, cell.x, cell.y, Part::Cell));
                }
            }
        }
        if game.piece_active() {
            let pa = game.piece_anim;
            let p = &game.player;
            let d = game.pair_draw();
            let halves = [(p.pivot, d.pivot, Part::Pivot), (p.satellite, d.satellite, Part::Satellite)];
            for (i, (color, (x, y), part)) in halves.into_iter().enumerate() {
                let key = CellKey { color, state: pa.state, ..Default::default() };
                let m = &mut self.piece[i];
                m.sync(&self.bank, pa.id, key, pa.age, self.bank.piece_script(color, pa.state));
                let (live, frame) = (m.live, m.player.frame);
                if let Some(f) = self.drawable(frame).filter(|_| live) {
                    out.push((f, x, y, part));
                }
            }
        } else {
            self.piece = [Mirror::default(); 2];
        }
        out
    }

    /// Arm spans in the order `func_800258A8` adds them (rows bottom up, columns 1–6, right
    /// side then left, joints before the arm), each at its cell in field pixels. Added later
    /// means drawn first, so draw this list reversed.
    pub(crate) fn span_frames(&mut self, game: &Game) -> Vec<(Frame, i32, i32, Part)> {
        let field = &game.field;
        let rows = field.rows();
        if self.spans.len() != COLS * rows * 2 {
            self.spans = vec![SpanMirror::default(); COLS * rows * 2];
        }
        let joints = field.arm_timer() >= 0x1F;
        let mut out = Vec::new();
        for r in (0..rows - 1).rev() {
            for c in 1..COLS - 1 {
                let cs = field.spans(c, r);
                let clut = i16::from(cs.color) - 1;
                for side in [RIGHT, LEFT] {
                    let span = &cs.sides[side];
                    let m = &mut self.spans[(c * rows + r) * 2 + side];
                    if !span.active() {
                        continue;
                    }
                    m.sync(&self.bank, span, side);
                    let (x, y) = (c as i32 * TILE, r as i32 * TILE);
                    if joints && cs.mode != -1 {
                        for (dc, p) in &m.segs {
                            if let Some(f) = Self::drawable_in(&self.bank, p.frame) {
                                out.push((f, x + dc * TILE, y, Part::Span { clut }));
                            }
                        }
                    }
                    if let Some(f) = Self::drawable_in(&self.bank, m.player.frame) {
                        out.push((f, x, y, Part::Span { clut }));
                    }
                }
            }
        }
        out
    }

    /// Current frames of the queued pairs, `[slot][satellite, pivot]` (`func_80022BF8`: every
    /// slot's scripts restart when the queue shifts, then step once per frame of play).
    pub(crate) fn queue_frames(&mut self, game: &Game) -> [[Option<Frame>; 2]; 3] {
        let (id, age) = (game.stats.pieces, game.queue_age);
        let mut out = [[None; 2]; 3];
        for (slot, &(pivot, satellite)) in game.queue.iter().enumerate() {
            for (half, color) in [satellite, pivot].into_iter().enumerate() {
                let key = CellKey { color, ..Default::default() };
                let m = &mut self.queue[slot][half];
                m.sync(&self.bank, id, key, age, self.bank.queue_script(color));
                let (live, frame) = (m.live, m.player.frame);
                out[slot][half] = if live { self.drawable(frame) } else { None };
            }
        }
        out
    }

    fn drawable(&self, ptr: u32) -> Option<Frame> {
        Self::drawable_in(&self.bank, ptr)
    }

    fn drawable_in(bank: &AnimBank, ptr: u32) -> Option<Frame> {
        let f = bank.frame(ptr)?;
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
