//! The playfield and its per-cell state machine (`func_800202E8` and friends).

use crate::tables::{ANIM_CLEAR, ANIM_GREY, ANIM_HOLD, ANIM_REACH, ANIM_RETRACT, ANIM_SPECIAL};

mod spans;
pub use spans::{CellSpans, Span, LEFT, RIGHT};

pub const COLS: usize = 8;
/// Field height `D_800FFD00` in the normal modes (19). Row `rows - 1` is the floor.
pub const ROWS: usize = 19;
/// Field height in Chain Reaction (mode 7, `func_80015ED4`).
pub const TALL_ROWS: usize = 79;
pub const TILE: i32 = 16;
pub const WALL: u8 = 0xFF;

/// Cell states (jump table `0x80010534`).
pub mod st {
    pub const EMPTY: u8 = 0;
    pub const FALLING: u8 = 1;
    pub const SETTLED: u8 = 2;
    pub const CLEARING: u8 = 4;
    pub const CLEARING_ARMS: u8 = 5;
    pub const CLEAR_SPECIAL: u8 = 7;
    pub const REACH: u8 = 8;
    pub const HOLD: u8 = 9;
    pub const RETRACT: u8 = 10;
    pub const SPECIAL_11: u8 = 11;
    pub const SPECIAL_12: u8 = 12;
    pub const SOLID_13: u8 = 13;
}

pub const ARM_LEFT: u8 = 1;
pub const ARM_RIGHT: u8 = 2;

/// Neighbour mask bits (`func_80022590`).
pub const MASK_DOWN: u8 = 1;
pub const MASK_LEFT: u8 = 2;
pub const MASK_RIGHT: u8 = 4;
pub const MASK_UP: u8 = 8;

/// Animation timer, a reduction of the script player in `func_80021D44`.
/// Only the total length matters to the rules: `done` is raised one frame after it elapses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Anim {
    len: Option<u16>,
    timer: i32,
    started: bool,
    pub done: bool,
    /// Frames since the animation started; for renderers.
    pub age: u16,
    /// Changes whenever the script restarts, so renderers can tell a restart from a continuation.
    pub id: u32,
}

impl Anim {
    fn for_state(state: u8, color: u8) -> Self {
        let len = if color == 0 || color == 6 {
            let table: &[(u8, u16)] = if color == 0 { &ANIM_GREY } else { &ANIM_SPECIAL };
            table.iter().find(|e| e.0 == state).map(|e| e.1)
        } else if color == WALL {
            None
        } else {
            match state {
                st::CLEARING | st::CLEARING_ARMS => Some(ANIM_CLEAR),
                st::REACH => Some(ANIM_REACH),
                st::HOLD => Some(ANIM_HOLD),
                st::RETRACT => Some(ANIM_RETRACT),
                _ => None,
            }
        };
        Anim { len, ..Default::default() }
    }

    fn step(&mut self) {
        if !self.started {
            self.started = true;
            self.timer = self.len.map_or(i32::MAX, i32::from);
            // A script that is only a done marker finishes on its first step.
            self.done |= self.timer == 0;
        } else if self.timer == 0 {
            self.done = true;
        }
        self.timer = self.timer.wrapping_sub(1);
        self.age = self.age.saturating_add(1);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub state: u8,
    pub color: u8,
    pending: u8,
    /// `+6`: sideways speed in px/frame, only set by the game-over burst.
    vx: i16,
    speed: i16,
    /// Pixel position; differs from the grid slot only while falling.
    pub x: i32,
    pub y: i32,
    falling: bool,
    just_landed: bool,
    /// `+0x2A == -1`: a neighbour was cleared, so this blob will reach out.
    pub excited: bool,
    /// Bit 0 = left arm, bit 1 = right arm (`+0x2C`).
    pub arms: u8,
    visited: bool,
    /// `+0x34`: same-colour settled neighbours, `MASK_*` bits. Picks the joined-blob sprites.
    pub mask: u8,
    /// `+0x32 == -1`: `mask` must be recomputed at the end of the frame.
    mask_dirty: bool,
    /// `+0x18`: entered state 10 last frame; its handler starts the retract spans.
    retract_new: bool,
    pub anim: Anim,
}

impl Cell {
    pub fn is_empty(&self) -> bool {
        self.state == st::EMPTY || self.color == 0
    }
}

/// Things the field reports back to the game during an update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Histogram(pub [u16; 14]);

impl Histogram {
    pub fn get(&self, state: u8) -> u16 {
        self.0[state as usize]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// `cells[column][row]`.
    cells: Vec<Vec<Cell>>,
    /// Arm spans per cell (`D_800C3538`), `spans[column][row]`.
    spans: Vec<Vec<CellSpans>>,
    /// `D_800B0060` / `D_800B1990`: grey blobs an arm has poked, waiting for player state 9 or 5.
    pokes: Vec<(usize, usize)>,
    /// `D_80100248`: frames the player has spent in states 5, 7 and 8 in a row.
    pub(crate) arm_timer: u16,
    /// `D_800FFDBC`: a reach span started this frame (sound 4).
    reach_sound: bool,
    /// `func_8002602C` played sound 8 this frame.
    poke_sound: bool,
    rows: usize,
    anim_serial: u32,
    /// `D_8010067C`: a coloured blob landed during this update (sound 3, `func_80021C68`).
    pub(crate) blob_landed: bool,
    /// `D_801005FA[p]`: grey/special blob count. + garbage dropped in, + colour-0 preset
    /// blobs, + 2 for a mode-7 □ lock; − 1 when a clear of colour 0 or 6 (or any state-7
    /// clear) finishes. Signed 16-bit and wrapping, as in the original.
    pub grey: i16,
    /// `D_80100634[p]`: coloured blob count. + 2 on every lock (`func_8001F67C`, whatever the
    /// colours), + non-zero preset blobs (colour 6 too); − 1 when a clear of colour 1–5 finishes.
    pub colored: i16,
}

impl Default for Field {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of one match check (`func_80033A14`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matches {
    pub group_sizes: Vec<u16>,
}

impl Matches {
    pub fn total(&self) -> u32 {
        self.group_sizes.iter().map(|&n| u32::from(n)).sum()
    }
}

impl Field {
    /// `func_8002277C`: empty field with walls in columns 0 and 7 and a floor row.
    pub fn new() -> Self {
        Self::with_rows(ROWS)
    }

    /// [`Field::new`] for height `rows` (`D_800FFD00`).
    pub fn with_rows(rows: usize) -> Self {
        let mut f = Field {
            cells: vec![vec![Cell::default(); rows]; COLS],
            spans: vec![vec![CellSpans::default(); rows]; COLS],
            pokes: Vec::new(),
            arm_timer: 0,
            reach_sound: false,
            poke_sound: false,
            rows,
            anim_serial: 0,
            blob_landed: false,
            grey: 0,
            colored: 0,
        };
        for c in 0..COLS {
            for r in 0..rows {
                f.reset(c, r, st::EMPTY, 0x80);
            }
        }
        for r in 0..rows - 1 {
            f.set_wall(0, r);
            f.set_wall(COLS - 1, r);
        }
        for c in 0..COLS {
            f.reset(c, rows - 1, st::SETTLED, WALL);
        }
        f
    }

    /// Height including the floor row.
    pub fn rows(&self) -> usize {
        self.rows
    }

    fn set_wall(&mut self, c: usize, r: usize) {
        let cell = &mut self.cells[c][r];
        cell.state = st::SETTLED;
        cell.color = WALL;
    }

    pub fn cell(&self, c: usize, r: usize) -> &Cell {
        &self.cells[c][r]
    }

    /// Tooling hook (tests that build fields by hand).
    #[doc(hidden)]
    pub fn cell_mut(&mut self, c: usize, r: usize) -> &mut Cell {
        &mut self.cells[c][r]
    }

    /// Cell lookup by signed coordinates; anything off the board reads as a wall.
    pub fn at(&self, c: i32, r: i32) -> Cell {
        if (0..COLS as i32).contains(&c) && (0..self.rows as i32).contains(&r) {
            self.cells[c as usize][r as usize]
        } else {
            Cell { state: st::SETTLED, color: WALL, ..Default::default() }
        }
    }

    /// `func_80022908`: reinitialise a cell.
    fn reset(&mut self, c: usize, r: usize, state: u8, color: u8) {
        self.cells[c][r] = Cell {
            state,
            color,
            x: c as i32 * TILE,
            y: r as i32 * TILE,
            speed: 0x80,
            ..Default::default()
        };
    }

    /// `func_8003B3DC`, at game over: every occupied cell of the play area gets a random
    /// burst velocity, vx = `rand % 8 − 4` px/frame and vy = `−(rand % 40) − 30` (1/128 px).
    /// Rows bottom to top, columns left to right, two `rand()` calls per cell.
    pub fn burst(&mut self, mut rand: impl FnMut() -> u32) {
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                let cell = &mut self.cells[c][r];
                if cell.state == st::EMPTY {
                    continue;
                }
                cell.vx = (rand() % 8) as i16 - 4;
                cell.speed = -((rand() % 40) as i16) - 30;
            }
        }
    }

    /// `func_8003B140`, every frame after game over: burst cells fly with gravity 0x20 until
    /// they are below the field (`y ≥ 16·19`), then become empty. Nothing else updates.
    pub fn burst_step(&mut self) {
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                let cell = &mut self.cells[c][r];
                if cell.state == st::EMPTY {
                    continue;
                }
                if cell.y < self.rows as i32 * TILE {
                    cell.x += i32::from(cell.vx);
                    cell.y += i32::from(cell.speed) / 128;
                    cell.speed = cell.speed.wrapping_add(0x20);
                } else {
                    self.reset(c, r, st::EMPTY, 0x80);
                }
            }
        }
    }

    /// `func_80027098`: a preset layout, one i16 per cell for rows 6…H−2, columns 1–6. 0x80
    /// leaves the cell empty; any other value is that colour, falling in (state 1, pending 1).
    /// Returns the number of blobs placed. Cells past the end of `layout` stay untouched.
    pub fn load_preset(&mut self, layout: &[i16]) -> u32 {
        let mut it = layout.iter();
        let mut placed = 0;
        for r in 6..self.rows - 1 {
            for c in 1..COLS - 1 {
                let Some(&v) = it.next() else { return placed };
                if v == 0x80 {
                    self.reset(c, r, st::EMPTY, 0x80);
                } else {
                    // Colour 0 counts as grey, anything else (6 too) as coloured.
                    if v == 0 {
                        self.grey = self.grey.wrapping_add(1);
                    } else {
                        self.colored = self.colored.wrapping_add(1);
                    }
                    self.drop_blob(c, r, v as u8);
                    placed += 1;
                }
            }
        }
        placed
    }

    /// `func_8003B54C`, a Time Trial record: every colour-6 blob (rows H−2 → 0) restarts in
    /// state 13, whose special-blob script is the pop.
    pub fn pop_special(&mut self) {
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                let cell = self.cells[c][r];
                if cell.state != st::EMPTY && cell.color == 6 {
                    self.cells[c][r].state = st::SOLID_13;
                    self.restart_anim(c, r);
                }
            }
        }
    }

    /// `func_8003B7E0`, the winner of a two-player set: every blob (grey too) turns colour 6
    /// and pops like [`Field::pop_special`].
    pub fn pop_all(&mut self) {
        for r in 0..self.rows - 1 {
            for c in 1..COLS - 1 {
                if self.cells[c][r].state != st::EMPTY {
                    self.cells[c][r].color = 6;
                }
            }
        }
        self.pop_special();
    }

    /// `func_8003B644`, every frame after a record: colour-6 blobs play their script and are
    /// emptied when it ends. Nothing else in the field moves. Returns how many emptied: the
    /// original takes them off player 1's grey count (`D_801005FE`) whichever field popped.
    pub fn pop_special_step(&mut self) -> i16 {
        let mut popped = 0;
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                let cell = &mut self.cells[c][r];
                if cell.state == st::EMPTY || cell.color != 6 {
                    continue;
                }
                cell.anim.step();
                if cell.anim.done {
                    self.reset(c, r, st::EMPTY, 0x80);
                    popped += 1;
                }
            }
        }
        popped
    }

    /// `func_800201C4`, a mode-14 bomb: every non-empty cell with a colour (rows H−2 → 0,
    /// columns 1–6, colour 6 included, grey not) gets excited.
    pub fn excite_all(&mut self) {
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                let cell = &mut self.cells[c][r];
                if cell.state != st::EMPTY && cell.color != 0 {
                    cell.excited = true;
                }
            }
        }
    }

    /// Place a settled blob (test and puzzle setup).
    pub fn put(&mut self, c: usize, r: usize, color: u8) {
        self.reset(c, r, st::SETTLED, color);
        let (c, r) = (c as i32, r as i32);
        for (dc, dr) in [(0, 0), (-1, 0), (1, 0), (0, 1), (0, -1)] {
            self.mark_dirty(c + dc, r + dr);
        }
    }

    /// Lock part of a piece into the field as a falling blob (`func_8001F67C`).
    pub fn drop_blob(&mut self, c: usize, r: usize, color: u8) {
        self.reset(c, r, st::FALLING, color);
        self.cells[c][r].pending = st::FALLING;
    }

    fn restart_anim(&mut self, c: usize, r: usize) {
        self.anim_serial = self.anim_serial.wrapping_add(1);
        let cell = &mut self.cells[c][r];
        cell.anim = Anim { id: self.anim_serial, ..Anim::for_state(cell.state, cell.color) };
    }

    fn mark_dirty(&mut self, c: i32, r: i32) {
        if (0..COLS as i32).contains(&c) && (0..self.rows as i32).contains(&r) {
            self.cells[c as usize][r as usize].mask_dirty = true;
        }
    }

    /// `func_800220A0`: recompute the neighbour masks of dirty blobs (rows `H-2 … 1`).
    /// A changed mask makes the original restart the cell's script and replay the frames spent in
    /// its current entry (`func_80021ED8`); `anim` is left alone because only the sprite changes.
    fn update_masks(&mut self) {
        for r in (1..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                let cell = self.cells[c][r];
                if !cell.mask_dirty || cell.color == WALL || cell.color == 0 || cell.color == 6 {
                    continue;
                }
                let joined = |n: &Cell| n.color == cell.color && n.state >= st::SETTLED;
                let mut mask = 0;
                if joined(&self.cells[c][r - 1]) {
                    mask |= MASK_UP;
                }
                if joined(&self.cells[c + 1][r]) {
                    mask |= MASK_RIGHT;
                }
                if joined(&self.cells[c - 1][r]) {
                    mask |= MASK_LEFT;
                }
                if joined(&self.cells[c][r + 1]) {
                    mask |= MASK_DOWN;
                }
                let n = &mut self.cells[c][r];
                n.mask_dirty = false;
                n.mask = mask;
            }
        }
    }

    /// One frame of `func_800202E8` for a player whose state machine is in `player_state`.
    pub fn update(&mut self, player_state: u8) -> Histogram {
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                self.apply_pending(c, r);
                self.run_state(c, r, player_state);
                if self.cells[c][r].state != st::EMPTY {
                    self.fall_step(c, r);
                    self.cells[c][r].anim.step();
                }
            }
        }
        let mut h = Histogram::default();
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                h.0[self.cells[c][r].state as usize] += 1;
            }
        }
        self.update_masks();
        h
    }

    /// `func_80020604`, first half.
    fn apply_pending(&mut self, c: usize, r: usize) {
        let cell = self.cells[c][r];
        match cell.pending {
            0 => return,
            st::FALLING => {
                self.reset(c, r, st::FALLING, cell.color);
                let n = &mut self.cells[c][r];
                n.excited = cell.excited;
                n.speed = 0x80;
                n.falling = true;
            }
            st::CLEARING => {
                // func_800208E8: a blob cleared while its arms retract also clears along them.
                let n = &mut self.cells[c][r];
                n.pending = 0;
                if cell.state != st::RETRACT {
                    n.state = st::CLEARING;
                } else {
                    n.state = st::CLEARING_ARMS;
                    self.restart_anim(c, r);
                    if cell.arms & ARM_LEFT != 0 {
                        self.spawn_span(c, r, -1, spans::mode::CLEARED);
                    }
                    if cell.arms & ARM_RIGHT != 0 {
                        self.spawn_span(c, r, 1, spans::mode::CLEARED);
                    }
                    return;
                }
            }
            p @ (6 | 7 | 11 | 12 | 13) => {
                let n = &mut self.cells[c][r];
                n.state = p;
                n.pending = 0;
            }
            _ => return,
        }
        self.restart_anim(c, r);
    }

    /// `func_80020604`, second half: per-state handlers.
    fn run_state(&mut self, c: usize, r: usize, player_state: u8) {
        match self.cells[c][r].state {
            st::SETTLED => self.settled(c, r, player_state),
            st::CLEARING | st::CLEARING_ARMS | st::CLEAR_SPECIAL => self.clearing(c, r),
            st::REACH => self.advance_when_done(c, r, st::HOLD),
            // func_800215A8 / func_8002166C: a poked grey blob clears once its wobble ends.
            st::SPECIAL_11 | st::SPECIAL_12 => self.advance_when_done(c, r, st::CLEARING),
            st::HOLD => {
                self.advance_when_done(c, r, st::RETRACT);
                let cell = &mut self.cells[c][r];
                cell.retract_new = cell.state == st::RETRACT;
            }
            st::RETRACT => {
                // func_8002146C: the first frame in state 10 starts the retract spans.
                let cell = self.cells[c][r];
                if cell.retract_new {
                    if cell.arms & ARM_LEFT != 0 {
                        self.spawn_span(c, r, -1, spans::mode::RETRACT);
                    }
                    if cell.arms & ARM_RIGHT != 0 {
                        self.spawn_span(c, r, 1, spans::mode::RETRACT);
                    }
                    self.cells[c][r].retract_new = false;
                }
                if self.cells[c][r].anim.done {
                    let cell = &mut self.cells[c][r];
                    cell.state = st::SETTLED;
                    cell.arms = 0;
                    self.restart_anim(c, r);
                }
            }
            _ => {}
        }
    }

    fn advance_when_done(&mut self, c: usize, r: usize, next: u8) {
        if self.cells[c][r].anim.done {
            self.cells[c][r].state = next;
            self.restart_anim(c, r);
        }
    }

    /// `func_80020A24`: an excited blob extends arms toward empty neighbours.
    fn settled(&mut self, c: usize, r: usize, player_state: u8) {
        let cell = self.cells[c][r];
        if !cell.excited {
            return;
        }
        if cell.color == 0 || cell.color == 6 {
            self.cells[c][r].excited = false;
            return;
        }
        if player_state != 7 {
            return;
        }
        self.cells[c][r].excited = false;
        self.cells[c][r].arms = 0;
        if self.cells[c + 1][r].is_empty() {
            self.cells[c][r].arms = ARM_RIGHT;
            self.spawn_span(c, r, 1, spans::mode::REACH);
        }
        if self.cells[c - 1][r].is_empty() {
            self.cells[c][r].arms += ARM_LEFT;
            self.spawn_span(c, r, -1, spans::mode::REACH);
        }
        self.cells[c][r].state = st::REACH;
        self.restart_anim(c, r);
    }

    /// `D_801005FA[p]` + `D_80100634[p]`: what the music director, the mascots and the CPU
    /// compare with their thresholds.
    pub fn blob_count(&self) -> i32 {
        i32::from(self.grey) + i32::from(self.colored)
    }

    /// `func_80020C58` / `func_80021030`: finish a clear, drop what is above, excite neighbours.
    fn clearing(&mut self, c: usize, r: usize) {
        let cell = self.cells[c][r];
        if !cell.anim.done {
            return;
        }
        self.reset(c, r, st::EMPTY, 0x80);
        // func_80021030 (state 7) always takes it off the grey count; func_80020C58 by colour.
        if cell.state == st::CLEAR_SPECIAL || cell.color == 0 || cell.color == 6 {
            self.grey = self.grey.wrapping_sub(1);
        } else {
            self.colored = self.colored.wrapping_sub(1);
        }
        for above in (0..r).rev() {
            let a = &mut self.cells[c][above];
            match a.state {
                st::EMPTY
                | st::CLEARING
                | st::CLEARING_ARMS
                | st::CLEAR_SPECIAL
                | st::SPECIAL_11
                | st::SPECIAL_12 => {}
                _ => {
                    a.pending = st::FALLING;
                    // Its row neighbours lose a joined partner.
                    for nc in [c - 1, c + 1] {
                        let n = &mut self.cells[nc][above];
                        if n.state == st::SETTLED && n.color != WALL {
                            n.mask_dirty = true;
                        }
                    }
                }
            }
        }
        if cell.state != st::CLEAR_SPECIAL && cell.color != 0 && cell.color != 6 {
            for (dx, dy) in crate::tables::DIR {
                let (nc, nr) = (c as i32 + dx / TILE, r as i32 + dy / TILE);
                if (0..COLS as i32).contains(&nc) && (0..self.rows as i32).contains(&nr) {
                    let n = &mut self.cells[nc as usize][nr as usize];
                    if n.state != st::EMPTY {
                        n.excited = true;
                    }
                }
            }
        }
    }

    /// `func_80033964`: is the slot below pixel position (x, y) solid?
    fn collides(&self, x: i32, y: i32) -> bool {
        let below = self.at(x / TILE, y / TILE + 1);
        below.state == st::SETTLED || below.state == st::SOLID_13
    }

    /// `func_80021804`: gravity for a falling blob.
    fn fall_step(&mut self, c: usize, r: usize) {
        let cell = self.cells[c][r];
        if !cell.falling {
            return;
        }
        let v = i32::from(cell.speed);
        let landed = if v / 128 >= 17 {
            let probe = cell.y + 16;
            self.cells[c][r].y = probe;
            if self.collides(cell.x, probe) {
                true
            } else {
                let y = probe - 16 + v / 128;
                self.cells[c][r].y = y;
                self.collides(cell.x, y)
            }
        } else {
            let y = cell.y + v / 127;
            self.cells[c][r].y = y;
            self.collides(cell.x, y)
        };
        if landed {
            self.land(c, r);
        } else {
            let s = &mut self.cells[c][r].speed;
            *s = s.wrapping_add(0x20);
        }
    }

    /// `func_8002193C`: move a falling blob into the slot where it came to rest.
    fn land(&mut self, c: usize, r: usize) {
        let cell = self.cells[c][r];
        let (nc, nr) = ((cell.x / TILE) as usize, (cell.y / TILE) as usize);
        self.reset(c, r, st::EMPTY, 0x80);
        self.reset(nc, nr, st::SETTLED, cell.color);
        if (1..=5).contains(&cell.color) {
            self.blob_landed = true;
        }
        let n = &mut self.cells[nc][nr];
        n.just_landed = true;
        n.excited = cell.excited;
        self.restart_anim(nc, nr);
        self.cells[nc][nr].anim.step();
        let (nc, nr) = (nc as i32, nr as i32);
        for (dc, dr) in [(0, 0), (-1, 0), (1, 0), (0, 1)] {
            self.mark_dirty(nc + dc, nr + dr);
        }
    }

    /// `func_80033A14`: find groups of 4+ and mark them for clearing.
    pub fn check_matches(&mut self) -> Matches {
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                self.cells[c][r].visited = false;
            }
        }
        let mut m = Matches::default();
        for r in (0..self.rows - 1).rev() {
            for c in 1..COLS - 1 {
                let cell = self.cells[c][r];
                if cell.state == st::EMPTY
                    || cell.color == 0
                    || (cell.arms == 0 && !cell.just_landed)
                    || cell.visited
                {
                    continue;
                }
                let n = self.flood(c, r);
                if n > 0 {
                    m.group_sizes.push(n);
                }
                self.cells[c][r].just_landed = false;
            }
        }
        m
    }

    /// `func_80033DFC`: flood fill from (c, r) following adjacency and arms.
    /// Returns the group size if it was 4 or more (and marks it), else 0.
    fn flood(&mut self, c: usize, r: usize) -> u16 {
        let color = self.cells[c][r].color;
        let mut list: Vec<(usize, usize)> = vec![(c, r)];
        self.cells[c][r].visited = true;
        let mut i = 0;
        while i < list.len() {
            let (c, r) = list[i];
            let (ci, ri) = (c as i32, r as i32);
            // Up / down neighbours, or arms passing over/under this blob.
            for dr in [-1, 1] {
                if !self.try_add(&mut list, ci, ri + dr, color) {
                    let n = self.at(ci, ri + dr);
                    if n.state == st::EMPTY || n.color == 0 {
                        if let Some(k) = self.scan(ci, ri + dr, 1) {
                            self.add_if_arm(&mut list, k, ri + dr, ARM_LEFT, color);
                        }
                        if let Some(k) = self.scan(ci, ri + dr, -1) {
                            self.add_if_arm(&mut list, k, ri + dr, ARM_RIGHT, color);
                        }
                    }
                }
                // The original handles "up" before right/left and "down" after; the order
                // only affects visiting order, never the resulting group.
            }
            // Right / left neighbours, or a blob further along whose arm reaches back.
            for (dc, arm_back) in [(1, ARM_LEFT), (-1, ARM_RIGHT)] {
                if !self.try_add(&mut list, ci + dc, ri, color) {
                    if let Some(k) = self.scan(ci, ri, dc) {
                        self.add_if_arm(&mut list, k, ri, arm_back, color);
                    }
                }
            }
            // This blob's own arms: the blob at the end, and blobs above/below the span.
            let arms = self.cells[c][r].arms;
            for (dc, bit) in [(1, ARM_RIGHT), (-1, ARM_LEFT)] {
                if arms & bit == 0 {
                    continue;
                }
                for s in 1..6 {
                    let k = ci + dc * s;
                    let n = self.at(k, ri);
                    if n.color == color && !n.visited {
                        self.mark(&mut list, k, ri);
                        break;
                    }
                    if n.state != st::EMPTY && n.color != 0 {
                        break;
                    }
                    self.try_add(&mut list, k, ri + 1, color);
                    self.try_add(&mut list, k, ri - 1, color);
                }
            }
            i += 1;
        }
        if list.len() < 4 {
            return 0;
        }
        for &(c, r) in &list {
            self.cells[c][r].pending = st::CLEARING;
        }
        list.len() as u16
    }

    fn mark(&mut self, list: &mut Vec<(usize, usize)>, c: i32, r: i32) {
        self.cells[c as usize][r as usize].visited = true;
        list.push((c as usize, r as usize));
    }

    fn try_add(&mut self, list: &mut Vec<(usize, usize)>, c: i32, r: i32, color: u8) -> bool {
        let n = self.at(c, r);
        if n.color == color && !n.visited && n.color != WALL {
            self.mark(list, c, r);
            true
        } else {
            false
        }
    }

    fn add_if_arm(&mut self, list: &mut Vec<(usize, usize)>, c: i32, r: i32, bit: u8, color: u8) {
        let n = self.at(c, r);
        if n.arms & bit != 0 && n.color == color && !n.visited {
            self.mark(list, c, r);
        }
    }

    /// First non-empty column after `c` in direction `dc` on row `r` (up to 5 steps).
    fn scan(&self, c: i32, r: i32, dc: i32) -> Option<i32> {
        let mut last = c;
        for s in 1..6 {
            last = c + dc * s;
            let n = self.at(last, r);
            if n.state != st::EMPTY && n.color != 0 {
                return Some(last);
            }
        }
        Some(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: usize = ROWS - 2; // bottom playable row

    fn seed(f: &mut Field, c: usize, r: usize) {
        f.cells[c][r].just_landed = true;
    }

    fn arms(f: &mut Field, c: usize, r: usize, a: u8) {
        f.cells[c][r].arms = a;
    }

    fn run(f: &mut Field, frames: usize) {
        for _ in 0..frames {
            f.update(1);
        }
    }

    #[test]
    fn four_in_a_column_match_and_clear() {
        let mut f = Field::new();
        for r in B - 3..=B {
            f.put(2, r, 1);
        }
        seed(&mut f, 2, B);
        let m = f.check_matches();
        assert_eq!(m.group_sizes, [4]);
        f.update(1);
        assert_eq!(f.cell(2, B).state, st::CLEARING);
        // Pending applied on update 1; the 47-frame script elapses on update 47, `done` is raised
        // on update 48 and the handler acts on update 49.
        run(&mut f, 47);
        assert_eq!(f.cell(2, B).state, st::CLEARING);
        run(&mut f, 1);
        assert_eq!(f.cell(2, B).state, st::EMPTY);
    }

    #[test]
    fn three_do_not_match() {
        let mut f = Field::new();
        for c in 1..=3 {
            f.put(c, B, 2);
        }
        seed(&mut f, 1, B);
        assert!(f.check_matches().group_sizes.is_empty());
    }

    #[test]
    fn non_seeds_are_not_checked() {
        let mut f = Field::new();
        for c in 1..=4 {
            f.put(c, B, 2);
        }
        assert!(f.check_matches().group_sizes.is_empty(), "no blob just landed or has arms");
    }

    #[test]
    fn arm_bridges_a_gap() {
        // 1 1 . 1 1 on the bottom row: two pairs joined by an arm across the gap.
        let mut f = Field::new();
        for c in [1, 2, 4, 5] {
            f.put(c, B, 3);
        }
        assert!(f.clone().check_matches().group_sizes.is_empty());
        arms(&mut f, 2, B, ARM_RIGHT);
        assert_eq!(f.check_matches().group_sizes, [4]);
    }

    #[test]
    fn blob_under_an_arm_connects() {
        // Row B-1: X . . where X (col 1) reaches right. Row B: . . Y Y Y under the arm span.
        let mut f = Field::new();
        f.put(1, B, 2); // support
        f.put(1, B - 1, 4);
        for c in 2..=4 {
            f.put(c, B, 4);
        }
        arms(&mut f, 1, B - 1, ARM_RIGHT);
        // X touches col2 (directly right of the support? no: col 2 row B is below the arm).
        assert_eq!(f.check_matches().group_sizes, [4]);
    }

    #[test]
    fn arm_from_the_other_side_is_seen_from_below() {
        let mut f = Field::new();
        f.put(5, B, 2);
        f.put(5, B - 1, 4); // reaches left over cols 1..4
        for c in 1..=3 {
            f.put(c, B, 4);
        }
        arms(&mut f, 5, B - 1, ARM_LEFT);
        seed(&mut f, 1, B);
        f.cells[5][B - 1].arms = ARM_LEFT;
        let m = f.check_matches();
        assert_eq!(m.group_sizes, [4]);
    }

    #[test]
    fn different_colour_stops_an_arm() {
        let mut f = Field::new();
        f.put(1, B, 1);
        f.put(2, B, 1);
        f.put(3, B, 5);
        f.put(4, B, 1);
        f.put(5, B, 1);
        arms(&mut f, 2, B, ARM_RIGHT);
        assert!(f.check_matches().group_sizes.is_empty());
    }

    #[test]
    fn falling_blob_accelerates_and_lands() {
        let mut f = Field::new();
        f.drop_blob(3, 2, 1);
        let mut frames = 0;
        while f.cell(3, B).state != st::SETTLED {
            f.update(1);
            frames += 1;
            assert!(frames < 200);
        }
        assert_eq!(f.cell(3, 2).state, st::EMPTY);
        // 15 rows = 240 px; speed starts at 1 px/frame and gains 0x20/128 px per frame.
        assert_eq!(frames, 42);
    }

    #[test]
    fn clearing_drops_and_excites_what_is_above() {
        let mut f = Field::new();
        for r in B - 3..=B {
            f.put(2, r, 1);
        }
        f.put(2, B - 4, 3);
        f.put(3, B, 3);
        seed(&mut f, 2, B);
        f.check_matches();
        run(&mut f, 49);
        assert_eq!(f.cell(2, B - 4).state, st::FALLING);
        assert!(f.cell(2, B - 4).excited);
        assert!(f.cell(3, B).excited, "right neighbour of a cleared blob");
        run(&mut f, 60);
        assert_eq!(f.cell(2, B).color, 3);
        assert!(f.cell(2, B).excited, "excitement survives the fall");
    }

    #[test]
    fn landing_joins_same_colour_neighbours() {
        let mut f = Field::new();
        f.put(2, B, 1);
        f.put(4, B, 2);
        f.drop_blob(3, B - 5, 1);
        for _ in 0..60 {
            f.update(1);
        }
        assert_eq!(f.cell(3, B).state, st::SETTLED);
        assert_eq!(f.cell(3, B).mask, MASK_LEFT);
        assert_eq!(f.cell(2, B).mask, MASK_RIGHT);
        assert_eq!(f.cell(4, B).mask, 0, "different colour");
        // A blob landing on top joins downward; the one below is marked too.
        f.drop_blob(3, B - 5, 1);
        for _ in 0..60 {
            f.update(1);
        }
        assert_eq!(f.cell(3, B - 1).mask, MASK_DOWN);
        assert_eq!(f.cell(3, B).mask, MASK_LEFT | MASK_UP);
    }

    #[test]
    fn excited_blobs_reach_hold_and_retract() {
        let mut f = Field::new();
        f.put(3, B, 2);
        f.cells[3][B].excited = true;
        f.update(7);
        assert_eq!(f.cell(3, B).state, st::REACH);
        assert_eq!(f.cell(3, B).arms, ARM_LEFT | ARM_RIGHT);
        let mut seen = vec![];
        for _ in 0..80 {
            f.update(8);
            let s = f.cell(3, B).state;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        assert_eq!(seen, [st::REACH, st::HOLD, st::RETRACT, st::SETTLED]);
        assert_eq!(f.cell(3, B).arms, 0);
    }
}
