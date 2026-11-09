//! Arm spans: the stretched arm a blob reaches across empty cells with (`D_800C3538`, 0x1C0
//! bytes per cell). Mostly drawing, but they carry three rules:
//!
//! - an arm passes over grey (colour 0) blobs and pokes each one its script reaches
//!   (`func_8002602C`): pending state 6, later 11 or 12, then cleared;
//! - when a blob is cleared while its arms retract, every cell under the arm clears the grey
//!   blobs around it and excites the coloured ones (`func_80034A7C`);
//! - after 30 frames of arms, the blobs an arm touches show as joined to it (`func_80026334`).

use super::{st, Field, COLS, MASK_DOWN, MASK_LEFT, MASK_RIGHT, MASK_UP, WALL};
use crate::tables::{span_index, SpanTiming, SPAN_SCRIPTS};

/// Index into [`CellSpans::sides`].
pub const LEFT: usize = 0;
pub const RIGHT: usize = 1;

/// The `mode` argument of `func_80024CE0`.
pub mod mode {
    /// Arms reaching out (`func_80020A24`, cell state 2 → 8).
    pub const REACH: i8 = 1;
    /// The blob was cleared while retracting (`func_800208E8`, state 10 → 5).
    pub const CLEARED: i8 = 0;
    /// Arms pulling back (`func_8002146C`, first frame of state 10).
    pub const RETRACT: i8 = -1;
}

/// One side's span. Inactive while `len` is 0 (`+6` left, `+7` right).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// Empty (or grey) cells between the blob and the first other blob or wall.
    pub len: u8,
    /// The spawn's mode (`mode::*`), which picked the script.
    pub mode: i8,
    /// `+4` / `+5`: how the far end looks, chosen from the blob it meets (−1, 0, 1).
    pub end: i8,
    /// `+0x29` / `+0xF5`: `len` when the far blob has this colour (and, reaching, no arms yet),
    /// else 0. The far blob is at `len + 1`.
    pub target: u8,
    /// Bit `k − 1`: the cell above / below span cell `k` has this colour (`+0x2A`/`+0x2B`,
    /// `+0xF6`/`+0xF7`).
    pub above: u8,
    pub below: u8,
    /// Steps the span script has taken (`func_80024C14`).
    pub steps: u16,
    /// Steps the joint segments have taken (in the draw, `func_80025A74`).
    pub seg_steps: u16,
    pub done: bool,
    /// Changes with every spawn, so renderers can tell a new span from a continuing one.
    pub id: u32,
}

impl Span {
    pub fn active(&self) -> bool {
        self.len != 0
    }

    fn timing(&self, side: usize) -> SpanTiming {
        SPAN_SCRIPTS[side][span_index(self.mode, self.end, self.len)]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellSpans {
    /// `+3`, shared by both sides: the latest spawn's mode. Segments are drawn only while it
    /// is not [`mode::RETRACT`].
    pub mode: i8,
    /// `+2`: the reaching blob's colour (its palette row is `color − 1`).
    pub color: u8,
    /// `[LEFT, RIGHT]`.
    pub sides: [Span; 2],
}

impl Field {
    pub fn spans(&self, c: usize, r: usize) -> &CellSpans {
        &self.spans[c][r]
    }

    /// `D_80100248`: the joint segments are drawn once this is ≥ 31.
    pub fn arm_timer(&self) -> u16 {
        self.arm_timer
    }

    fn slot(&self, c: i32, r: i32) -> Option<(usize, usize)> {
        ((0..COLS as i32).contains(&c) && (0..self.rows as i32).contains(&r)).then_some((c as usize, r as usize))
    }

    /// `func_80024CE0(c, r, dir, mode)`: scan the row from (c, r) toward `dir` and start that
    /// side's span.
    pub(super) fn spawn_span(&mut self, c: usize, r: usize, dir: i32, mode: i8) {
        let color = self.cells[c][r].color;
        let (mut above, mut below, mut target) = (0u8, 0u8, 0u8);
        let mut k = 1;
        let far = loop {
            let tc = c as i32 + k * dir;
            let t = self.at(tc, r as i32);
            if k >= 7 || !(t.state == st::EMPTY || t.color == 0) {
                if t.color == color {
                    match mode {
                        mode::CLEARED => target = (k - 1) as u8,
                        mode::REACH => target = if t.arms != 0 { 0 } else { (k - 1) as u8 },
                        mode::RETRACT => self.mark_dirty(tc, r as i32),
                        _ => {}
                    }
                }
                break t;
            }
            // An empty or grey cell under the arm.
            let bit = 1 << (k - 1);
            for (dr, mask) in [(1, &mut below), (-1, &mut above)] {
                let (nr, nc) = (r as i32 + dr, tc);
                if self.slot(nc, nr).is_some_and(|(x, y)| self.cells[x][y].color == color) {
                    *mask |= bit;
                    if mode == mode::RETRACT {
                        self.mark_dirty(nc, nr);
                    }
                }
            }
            if mode == mode::CLEARED {
                self.grey_burst(tc as usize, r);
            }
            k += 1;
        };
        let end = match mode {
            mode::REACH => {
                self.reach_sound = true;
                if far.color != WALL && (far.arms != 0 || far.excited) {
                    -1
                } else {
                    0
                }
            }
            mode::CLEARED => {
                if far.color == WALL || far.arms == 0 {
                    0
                } else if far.state == st::CLEARING_ARMS || far.pending == st::CLEARING {
                    -1
                } else {
                    1
                }
            }
            mode::RETRACT => {
                if far.state == st::SETTLED {
                    0
                } else if far.state == st::CLEARING_ARMS || far.pending == st::CLEARING {
                    1
                } else {
                    -1
                }
            }
            _ => 0,
        };
        self.anim_serial = self.anim_serial.wrapping_add(1);
        let side = usize::from(dir > 0);
        let cs = &mut self.spans[c][r];
        cs.mode = mode;
        cs.color = color;
        cs.sides[side] = Span {
            len: (k - 1) as u8,
            mode,
            end,
            target,
            above,
            below,
            id: self.anim_serial,
            ..Default::default()
        };
        if mode == mode::RETRACT {
            // The end of func_80024CE0 steps a retract span once straight away.
            self.step_span(c, r, side);
        }
    }

    /// `func_80034A7C`, for an empty or grey cell under the arm of a cleared blob: settled grey
    /// neighbours start clearing (state 7), coloured neighbours become excited.
    fn grey_burst(&mut self, c: usize, r: usize) {
        let near = [(0, 1), (0, -1), (1, 0), (-1, 0)];
        for (dc, dr) in near {
            if let Some((x, y)) = self.slot(c as i32 + dc, r as i32 + dr) {
                let n = self.cells[x][y];
                if n.state == st::SETTLED && n.color == 0 {
                    self.cells[x][y].state = st::CLEAR_SPECIAL;
                    self.restart_anim(x, y);
                    self.cells[x][y].anim.step();
                }
            }
        }
        for (dc, dr) in near {
            if let Some((x, y)) = self.slot(c as i32 + dc, r as i32 + dr) {
                let n = &mut self.cells[x][y];
                if !matches!(n.color, 0 | 0x80 | WALL) {
                    n.excited = true;
                }
            }
        }
    }

    /// `func_80024B48` for one side: retire a finished span, else step its script and act on
    /// the event it raises.
    fn step_span(&mut self, c: usize, r: usize, side: usize) {
        let span = self.spans[c][r].sides[side];
        if span.done {
            self.spans[c][r].sides[side] = Span { id: span.id, ..Default::default() };
            return;
        }
        let timing = span.timing(side);
        let step = span.steps + 1;
        let s = &mut self.spans[c][r].sides[side];
        s.steps = step;
        s.done = step >= u16::from(timing.done);
        if let Some(&(_, n)) = timing.events.iter().find(|e| u16::from(e.0) == step) {
            self.poke(c, r, side, n);
        }
    }

    /// `func_8002602C`: the arm has reached `n` cells out; a settled grey blob there is poked.
    fn poke(&mut self, c: usize, r: usize, side: usize, n: u8) {
        let tc = if side == RIGHT { c as i32 + i32::from(n) } else { c as i32 - i32::from(n) };
        let Some((x, y)) = self.slot(tc, r as i32) else { return };
        let t = &mut self.cells[x][y];
        if t.state == st::SETTLED && t.color == 0 {
            t.pending = 6;
            self.poke_sound = true;
            self.pokes.push((x, y));
        }
    }

    /// `func_80026334`, 30 frames into the arms: the far blob (if it has this colour) and the
    /// blobs above and below the span show as joined to the arm.
    fn join(&mut self, c: usize, r: usize, side: usize) {
        let span = self.spans[c][r].sides[side];
        let dir = if side == RIGHT { 1 } else { -1 };
        let (far_bit, c, r) = (if side == RIGHT { MASK_LEFT } else { MASK_RIGHT }, c as i32, r as i32);
        let or = |f: &mut Field, x: i32, y: i32, bit: u8| {
            if let Some((x, y)) = f.slot(x, y) {
                f.cells[x][y].mask |= bit;
            }
        };
        if span.target != 0 {
            or(self, c + dir * (i32::from(span.target) + 1), r, far_bit);
        }
        for k in 1..6 {
            if span.below >> (k - 1) & 1 != 0 {
                or(self, c + dir * k, r + 1, MASK_UP);
            }
        }
        for k in 1..6 {
            if span.above >> (k - 1) & 1 != 0 {
                or(self, c + dir * k, r - 1, MASK_DOWN);
            }
        }
    }

    /// `func_800248F4`, after the cell update: step every span, hand poked grey blobs on once
    /// the player reaches state 9 (pending 11) or 5 (pending 12), and count the arm frames.
    /// Returns (a reach started, a grey blob was poked) for sounds 4 and 8.
    pub fn step_spans(&mut self, player_state: u8) -> (bool, bool) {
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                for side in [RIGHT, LEFT] {
                    if self.spans[c][r].sides[side].active() {
                        self.step_span(c, r, side);
                        if self.arm_timer == 30 {
                            self.join(c, r, side);
                        }
                    }
                }
            }
        }
        // func_80026184
        let pending = match player_state {
            9 => Some(11),
            5 => Some(12),
            _ => None,
        };
        if let Some(p) = pending {
            for (x, y) in std::mem::take(&mut self.pokes) {
                self.cells[x][y].pending = p;
            }
        }
        self.arm_timer = if matches!(player_state, 5 | 7 | 8) { self.arm_timer.wrapping_add(1) } else { 0 };
        (std::mem::take(&mut self.reach_sound), std::mem::take(&mut self.poke_sound))
    }

    /// The stepping half of `func_800258A8` (run with the field draw, also while paused or
    /// over): once the arms have been out 31 frames, joint segments animate.
    pub fn step_span_segments(&mut self) {
        if self.arm_timer < 0x1F {
            return;
        }
        for col in self.spans.iter_mut() {
            for cs in col.iter_mut() {
                if cs.mode == mode::RETRACT {
                    continue;
                }
                for s in cs.sides.iter_mut().filter(|s| s.active()) {
                    s.seg_steps = s.seg_steps.wrapping_add(1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{ARM_RIGHT, ROWS};

    const B: usize = ROWS - 2;

    /// One frame of the cell update and the span step, as `Game::step` runs them.
    fn frame(f: &mut Field, player_state: u8) -> (bool, bool) {
        f.update(player_state);
        f.step_spans(player_state)
    }

    #[test]
    fn reach_spans_the_gap_and_plays_the_reach_sound() {
        let mut f = Field::new();
        f.put(1, B, 1);
        f.put(5, B, 1);
        f.cells[1][B].excited = true;
        let (reach, _) = frame(&mut f, 7);
        assert!(reach);
        assert_eq!(f.cell(1, B).state, st::REACH);
        let span = f.spans(1, B).sides[RIGHT];
        // Columns 2–4 are empty; column 5 has the same colour and no arms.
        assert_eq!((span.len, span.mode, span.end, span.target), (3, mode::REACH, 0, 3));
        assert_eq!(span.steps, 1);
        assert!(!f.spans(1, B).sides[LEFT].active(), "the wall is right next to it");
    }

    #[test]
    fn span_retires_the_step_after_its_script_is_done() {
        let mut f = Field::new();
        f.put(1, B, 1);
        f.cells[1][B].excited = true;
        frame(&mut f, 7);
        // Keep the blob from holding and retracting, which would start a new span.
        f.cells[1][B].state = st::SETTLED;
        // Reaching right across five empty cells to the wall: done on step 60.
        let done = SPAN_SCRIPTS[RIGHT][span_index(mode::REACH, 0, 5)].done;
        for _ in 1..done {
            frame(&mut f, 7);
        }
        assert!(f.spans(1, B).sides[RIGHT].done);
        frame(&mut f, 7);
        assert!(!f.spans(1, B).sides[RIGHT].active());
    }

    #[test]
    fn arm_reaches_over_a_grey_blob_and_it_is_cleared() {
        let mut f = Field::new();
        f.put(1, B, 1);
        f.put(2, B, 0);
        f.cells[1][B].excited = true;
        let mut poked = false;
        for _ in 0..20 {
            poked |= frame(&mut f, 7).1;
        }
        // Grey counts as empty for the arm, which pokes column 2 at its first event.
        assert_eq!(f.spans(1, B).sides[RIGHT].len, 5);
        assert!(poked);
        assert_eq!(f.cell(2, B).state, 6);
        // Player state 9 hands it on after the cell update (pending 11), then 11 runs its
        // 20 frames, clears and is gone.
        frame(&mut f, 9);
        assert_eq!(f.cell(2, B).state, 6);
        frame(&mut f, 9);
        assert_eq!(f.cell(2, B).state, st::SPECIAL_11);
        for _ in 0..30 {
            frame(&mut f, 9);
        }
        assert_eq!(f.cell(2, B).state, st::EMPTY);
    }

    #[test]
    fn clearing_while_retracting_clears_grey_under_the_arm() {
        let mut f = Field::new();
        f.put(1, B, 1);
        f.put(3, B, 0);
        f.put(4, B, 0);
        f.put(3, B - 1, 2);
        let x = &mut f.cells[1][B];
        x.state = st::RETRACT;
        x.arms = ARM_RIGHT;
        x.pending = st::CLEARING;
        f.cells[3][B - 1].excited = false;
        frame(&mut f, 8);
        assert_eq!(f.cell(1, B).state, st::CLEARING_ARMS);
        assert_eq!(f.spans(1, B).sides[RIGHT].mode, mode::CLEARED);
        // Grey blobs next to the arm's cells start their clear; the blob above it is excited.
        assert_eq!(f.cell(3, B).state, st::CLEAR_SPECIAL);
        assert_eq!(f.cell(4, B).state, st::CLEAR_SPECIAL);
        assert!(f.cell(3, B - 1).excited);
    }

    #[test]
    fn joins_show_after_thirty_frames_of_arms() {
        let mut f = Field::new();
        f.put(1, B, 1);
        f.put(4, B, 1);
        f.put(2, B - 1, 2);
        f.put(3, B - 1, 1);
        f.cells[1][B].excited = true;
        f.update(0);
        f.arm_timer = 29;
        frame(&mut f, 7);
        let span = f.spans(1, B).sides[RIGHT];
        assert_eq!((span.len, span.target, span.above), (2, 2, 0b10));
        assert_eq!(f.cell(4, B).mask & MASK_LEFT, 0);
        frame(&mut f, 7);
        assert_ne!(f.cell(4, B).mask & MASK_LEFT, 0, "far blob joined from the left");
        assert_ne!(f.cell(3, B - 1).mask & MASK_DOWN, 0, "blob above the span joined downward");
        assert_eq!(f.cell(2, B - 1).mask & MASK_DOWN, 0, "other colour");
    }
}
