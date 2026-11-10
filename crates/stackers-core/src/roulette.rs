//! The two-player HUD strip above each field (`func_8003BCD0`; modes 2, 13, 14/15): the
//! special-column indicator ("roulette") and the per-column incoming-garbage digits.
//!
//! Both are driven by `BLT_ANM.OBJ` scripts. The indicator's script has ops that change gameplay
//! (the special column), so it is interpreted here from [`INDICATOR_SCRIPTS`], a transcription of
//! the data (`*(0x801C0044)`, checked by `stackers-web/tests/roulette.rs`). The digits only need
//! their timing; the web layer replays the frames.

/// One command of an indicator script (`func_8003BD78`, `jtbl_80012024`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// A `(frames, frame)` pair: the script rests here for that many frames.
    Show(u8),
    /// Op 0: continue at the start of indicator script n.
    Jump(u8),
    /// Op 2: the indicator's palette (`D_800C3134`).
    Clut(u8),
    /// Op 3: the special column steps right (wrapping 6 → 1), SE 0x19, the sparkle restarts.
    Step,
    /// Op 4: the sparkle's sideways bob takes the next value of [`WOBBLE`].
    Wobble,
}

use Op::*;

/// `*(0x801C0044)`: 0 idle (bobbing), 1 spinning, 2 the stop flash (then idle). Every frame
/// record of these is the same 32×40 arrow (the draw uses `D_800C3120`, not the script frame).
pub const INDICATOR_SCRIPTS: [&[Op]; 3] = [
    &[Clut(4), Show(1), Wobble, Jump(0)],
    &[
        Clut(2), Step, Show(5), Clut(3), Step, Show(5), Clut(4), Step, Show(5), Clut(5), Step, Show(5), Jump(1),
    ],
    &[
        Clut(4), Show(2), Clut(5), Show(2), Clut(2), Show(2), Clut(3), Show(2),
        Clut(4), Show(2), Clut(5), Show(2), Clut(2), Show(2), Clut(3), Show(2),
        Clut(4), Show(2), Clut(5), Show(2), Clut(2), Show(2), Clut(3), Show(2),
        Clut(4), Show(2), Clut(5), Show(2), Clut(2), Show(2), Clut(3), Show(2),
        Jump(0),
    ],
];

/// `D_800557F4`: the sparkle's x offset, one value per op 4 (the 0x80 end marker restarts at
/// index 1 with value `[0]`).
pub const WOBBLE: [i8; 68] = [
    0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1,
    1, 1, 0, 0, 0, 0, 0, 0, -1, -1, -1, -1, -1, -1, -1, -1, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2,
    -2, -2, -1, -1, -1, -1, -1, -1, -1, -1,
];

/// Indicator palette before any op 2 (`func_8003B958`).
const START_CLUT: u8 = 4;

/// `D_800C3110 + 0x30·p`: one player's special-column indicator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roulette {
    /// `D_800C3132`: 0–5, the special column minus 1.
    pub column: u8,
    /// `D_800C313C`: the script last started (0 idle, 1 spinning, 2 flash). Script 2 jumping
    /// into 0 leaves this at 2.
    pub script: u8,
    /// `D_800C3134`: the arrow's palette (2–5).
    pub clut: u8,
    /// `D_801003D4[p]`: the sparkle's x offset.
    pub wobble: i8,
    /// `D_800C313A`: next index into [`WOBBLE`].
    wobble_at: u8,
    /// The op list being run and the next op in it.
    code: u8,
    pc: u8,
    /// `+8`: frames left on the current pair (0 = read on).
    count: u8,
    /// Frames stepped since the script was last started (the sparkle and the message replay
    /// their scripts from this).
    pub timer: u16,
}

impl Default for Roulette {
    fn default() -> Self {
        Roulette::new()
    }
}

impl Roulette {
    /// `func_8003B958`: column 2, idle script, palette 4.
    pub fn new() -> Self {
        Roulette { column: 1, script: 0, clut: START_CLUT, wobble: 0, wobble_at: 0, code: 0, pc: 0, count: 0, timer: 0 }
    }

    /// `func_8003C4B4(p, n)`: the script restarts from its first op on the next step.
    pub fn start(&mut self, script: u8) {
        self.script = script;
        self.code = script;
        self.pc = 0;
        self.count = 0;
        self.timer = 0;
    }

    /// One frame of `func_8003BD78`; returns whether op 3 ran (SE 0x19).
    pub fn step(&mut self) -> bool {
        let mut stepped = false;
        self.timer = self.timer.saturating_add(1);
        if self.count == 0 {
            // Bounded like the original's data: every loop has a Show.
            for _ in 0..64 {
                let ops = INDICATOR_SCRIPTS[usize::from(self.code)];
                let op = ops[usize::from(self.pc)];
                self.pc += 1;
                match op {
                    Show(n) => {
                        self.count = n;
                        break;
                    }
                    Jump(s) => {
                        self.code = s;
                        self.pc = 0;
                    }
                    Clut(c) => self.clut = c,
                    Step => {
                        self.column = (self.column + 1) % 6;
                        self.wobble = 0;
                        stepped = true;
                    }
                    Wobble => {
                        let mut i = usize::from(self.wobble_at);
                        let mut v = WOBBLE.get(i).copied();
                        i += 1;
                        if v.is_none() {
                            // The 0x80 marker: back to index 1 with the first value.
                            v = Some(WOBBLE[0]);
                            i = 1;
                        }
                        self.wobble_at = i as u8;
                        self.wobble = v.unwrap_or(0);
                    }
                }
            }
        }
        self.count = self.count.saturating_sub(1);
        stepped
    }
}

/// Frames a column's old digit takes to roll out (`*(0x801C003C)` entries 31–59: eight 1-frame
/// pairs, then op 5; entry 30, an old count of 0, is op 5 at once).
pub const ROLL_OUT_FRAMES: u16 = 8;
/// Counts above this show as this (`func_8003C5A0`/`func_8003C1AC` clamp to 0x1D).
pub const TALLY_MAX: u8 = 29;

/// One column's incoming-garbage digit (`D_801036B0 + 0x90·p + 0x18·c`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnTally {
    /// `D_800C2FE8`: incoming + pending as of last frame.
    pub last: i16,
    /// The digit rolled in (`D_801036C0`), 0–29.
    pub shown: u8,
    /// While the old digit rolls out, that digit (script entry 30 + it).
    pub leaving: Option<u8>,
    /// Steps of the current script (0 = not started: nothing to draw).
    pub age: u16,
    /// `D_801036C4`: palette of the digit, by count (set when one rolls in).
    pub clut: u8,
}

/// `func_8003C698`: 8 for 0–2 blobs, 7 for 3–5, 6 for 6 and more.
pub fn tally_clut(n: u8) -> u8 {
    match n {
        0..=2 => 8,
        3..=5 => 7,
        _ => 6,
    }
}

fn clamp(total: i16) -> u8 {
    total.clamp(0, i16::from(TALLY_MAX)) as u8
}

impl ColumnTally {
    /// `func_8003B958`: the current count, rolling in on the first step.
    pub fn new(total: i16) -> Self {
        let shown = clamp(total);
        ColumnTally { last: total, shown, leaving: None, age: 0, clut: tally_clut(shown) }
    }

    /// `func_8003C5A0` then `func_8003C1AC` for one frame of play.
    pub fn step(&mut self, total: i16) {
        if total != self.last {
            self.leaving = Some(clamp(self.last));
            self.age = 0;
        }
        self.last = total;
        self.age = self.age.saturating_add(1);
        if let Some(old) = self.leaving {
            let len = if old == 0 { 0 } else { ROLL_OUT_FRAMES };
            if self.age > len {
                // Op 5: roll the current count in, loading its first frame this step.
                self.leaving = None;
                self.shown = clamp(total);
                self.clut = tally_clut(self.shown);
                self.age = 1;
            }
        }
    }

    /// `(script entry of *(0x801C003C), steps taken)` to replay for this frame.
    pub fn script(&self) -> (u8, u16) {
        match self.leaving {
            Some(old) => (30 + old, self.age),
            None => (self.shown, self.age),
        }
    }
}

/// Both players' indicators and digits (`func_8003BCD0`), as the two-player modes keep them.
pub type Strip = ([Roulette; 2], [[ColumnTally; 6]; 2]);

/// `func_8003B958` at the start of a set: both indicators on column 2, each column's digit
/// showing its incoming + pending count. Points the players' special columns at the indicators.
pub fn new_strip(v: &mut crate::Versus) -> Strip {
    let roulette = [Roulette::new(); 2];
    let tallies = [0, 1].map(|p| {
        let g = &v.games[p].garbage;
        std::array::from_fn(|c| ColumnTally::new(g.incoming[c + 1] + g.pending[c + 1]))
    });
    sync_columns(&roulette, v);
    (roulette, tallies)
}

/// The players' special columns (1–6) follow the indicators.
pub fn sync_columns(roulette: &[Roulette; 2], v: &mut crate::Versus) {
    for (g, r) in v.games.iter_mut().zip(roulette) {
        g.special_column = Some(usize::from(r.column) + 1);
    }
}

/// `func_8003BCD0`'s steps in the play sub-state, after the frame's player and cell updates:
/// a chain reaching 2 spins the indicator (`func_8003C340`), the turn ending or ○/✕ stops it
/// (`func_8003C3D8`), and the digits follow the garbage counters (`func_8003C5A0`,
/// `func_8003C1AC`). The roulette's SE goes into `sounds` once.
pub fn step_strip(
    roulette: &mut [Roulette; 2],
    tallies: &mut [[ColumnTally; 6]; 2],
    v: &mut crate::Versus,
    pressed: [u16; 2],
    sounds: &mut Vec<u8>,
) {
    use crate::game::{pad, ps, se};
    for p in 0..2 {
        let g = &v.games[p];
        let r = &mut roulette[p];
        if g.stats.chain == 2 && g.chain_before == 1 {
            r.start(1);
        }
        if r.script == 1 && (g.player.state == ps::NO_MATCH || pressed[p] & (pad::CIRCLE | pad::CROSS) != 0) {
            r.start(2);
        }
        if r.step() && !sounds.contains(&se::ROULETTE) {
            sounds.push(se::ROULETTE);
        }
        let gb = &g.garbage;
        for (c, t) in tallies[p].iter_mut().enumerate() {
            t.step(gb.incoming[c + 1] + gb.pending[c + 1]);
        }
    }
    sync_columns(roulette, v);
}

/// A set ended: a spinning indicator stops (`func_8003C3D8`'s script 2).
pub fn stop_spins(roulette: &mut [Roulette; 2]) {
    for r in roulette {
        if r.script == 1 {
            r.start(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spin_steps_on_its_first_frame_then_every_5() {
        let mut r = Roulette::new();
        r.start(1);
        let steps: Vec<usize> = (0..21).filter(|_| r.step()).collect();
        assert_eq!(steps.len(), 5);
        let mut r = Roulette::new();
        r.start(1);
        let mut at = vec![];
        for _ in 0..21 {
            if r.step() {
                at.push(r.timer);
            }
        }
        assert_eq!(at, [1, 6, 11, 16, 21]);
        assert_eq!(r.column, (1 + 5) % 6);
        assert_eq!(r.clut, 2, "the fifth step starts the palette cycle again");
    }

    #[test]
    fn the_flash_lasts_32_frames_then_bobs() {
        let mut r = Roulette::new();
        r.start(2);
        for _ in 0..32 {
            r.step();
            assert_eq!(r.wobble, 0);
        }
        assert_eq!(r.code, 2);
        r.step();
        assert_eq!((r.code, r.script, r.clut), (0, 2, 4));
        // Idle: op 4 each frame after the first.
        let mut r = Roulette::new();
        let mut seen = vec![];
        for _ in 0..80 {
            r.step();
            seen.push(r.wobble);
        }
        assert_eq!(&seen[..1], &[0]);
        assert_eq!(&seen[1..69], &WOBBLE[..]);
        assert_eq!(seen[69], WOBBLE[0], "the end marker restarts at index 1 with value [0]");
        assert_eq!(seen[70], WOBBLE[1]);
    }

    #[test]
    fn a_digit_rolls_out_for_8_frames_then_the_new_count_rolls_in() {
        let mut c = ColumnTally::new(0);
        c.step(0);
        assert_eq!(c.script(), (0, 1));
        c.step(4);
        assert_eq!((c.script(), c.clut), ((4, 1), 7), "an old 0 rolls in the new count at once");
        for _ in 0..5 {
            c.step(4);
        }
        c.step(7);
        assert_eq!(c.script(), (34, 1));
        for k in 2..=8 {
            c.step(7);
            assert_eq!(c.script(), (34, k));
        }
        assert_eq!(c.clut, 7, "the palette changes only when the new digit rolls in");
        c.step(40);
        assert_eq!(c.script(), (37, 1), "a change mid-roll rolls out the latest count");
        for _ in 0..8 {
            c.step(40);
        }
        assert_eq!((c.script(), c.clut), ((29, 1), 6));
    }
}
