//! The computer opponent (`func_80034C60`, docs "CPU").
//!
//! The AI runs inside the pad read (`func_80014F20`), before the frame's player updates, and its
//! output replaces the CPU player's held buttons (Start and Select still come from the pad).
//! While the player waits to spawn (state 11) or counts down the intro (state 2) it plans: it
//! picks a strategy mode from the blob count (`func_80035160`), files every column × orientation
//! into six priority lists (`func_800353A0`) and takes a target from the first non-empty list in
//! the mode's order (`func_80037A00`). While the piece is under control (state 0) it rotates,
//! then steps sideways, toward the target (`func_80038490`). In every other state the last
//! output is held.

use crate::field::{st, Field, WALL};
use crate::game::{pad, ps, Game};
use crate::rng::BiosRand;
use crate::tables::DIR;

/// A 12-byte record of `D_8005588C` (`[variant][stage]`, copied to `D_800B13E8 + 12·p`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Params {
    /// `+0`: how a list entry is chosen (0 random, 1 first or last, 2 deepest). Always 2.
    pub select: i16,
    /// `+2`: bit 0 = leave mode 1 when 4+ garbage blobs are pending, bit 1 = soft-drop once
    /// aligned, bit 2 = skip the delay when cell (3, 9) is settled.
    pub flags: u16,
    /// `+4`: blob count that switches the modes.
    pub blobs: i16,
    /// `+6`: compared with a third count that is always 0 (`D_80100638`).
    pub third: i16,
    /// `+8`: soft-drop only below this y (px).
    pub drop_y: i16,
    /// `+0xA`: frames to wait after spawning before moving (modes 1 and 2).
    pub delay: i16,
}

impl Params {
    /// The record for a level table variant (0 or 1) and stage (0–13).
    pub fn stage(variant: usize, stage: usize) -> Self {
        let [select, flags, blobs, third, drop_y, delay] = crate::tables::CPU_PARAMS[variant][stage];
        Params { select, flags: flags as u16, blobs, third, drop_y, delay }
    }
}

/// One candidate (`D_80066608` etc., 12 bytes: the top cell's x and y, then a rotation).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Target {
    pub x: i32,
    pub y: i32,
    pub rot: u8,
}

/// Room for 16 entries per list (stride 0xC4): at most 6 up + 5 left + 5 right fit one list.
const LIST_LEN: usize = 16;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct List {
    len: usize,
    items: [Target; LIST_LEN],
}

impl List {
    fn push(&mut self, t: Target) {
        self.items[self.len] = t;
        self.len += 1;
    }

    fn entries(&self) -> &[Target] {
        &self.items[..self.len]
    }
}

/// List order per mode (`func_80037A90`, `func_80037BAC`, `func_80037CC8`). The lists are
/// `D_80066608`, `D_80066940`, `D_80066480`, `D_80066BB0`, `D_800667B8`, `D_800662F8`.
const ORDER: [[usize; 6]; 3] = [[0, 1, 2, 3, 4, 5], [3, 4, 5, 1, 2, 0], [1, 2, 0, 3, 4, 5]];

/// Which list each satellite result (0 forms three, 1 touches, 2 neither) goes to, by how the
/// pivot fares (`func_800366A0` forms three, `func_80036928` touches, `func_80036BB0` neither).
const TIERS: [[usize; 3]; 3] = [[0, 1, 2], [1, 3, 4], [2, 4, 5]];

/// One CPU player's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cpu {
    pub params: Params,
    /// `D_800B1428[p]`: 0 build, 1 stack, 2 clear (named for the list orders they use).
    pub mode: i16,
    /// `D_801005E8[p]`: frames left before the piece moves.
    pub delay: i16,
    /// `D_8010ADA8[p]`: the idle wiggle's countdown. Never reset by the game (0 at boot).
    pub wiggle: i16,
    pub target: Target,
    /// `D_800C2F58[p]`: the buttons the CPU holds.
    pub pad: u16,
    lists: [List; 6],
}

impl Cpu {
    /// `func_80034F74`: mode 1 and no buttons. The wiggle countdown carries over (`wiggle`).
    pub fn new(params: Params, wiggle: i16) -> Self {
        Cpu {
            params,
            mode: 1,
            delay: 0,
            wiggle,
            target: Target::default(),
            pad: 0,
            lists: Default::default(),
        }
    }

    /// One frame for the player `g`: returns the held buttons to feed it. `rand` is the shared
    /// BIOS `rand()`; the list choice and the idle wiggle consume it.
    pub fn step(&mut self, g: &Game, rand: &mut BiosRand) -> u16 {
        match g.player.state {
            ps::CONTROL => {
                self.drive(g, rand);
            }
            ps::SPAWN | ps::INTRO => {
                self.switch_mode(field_blobs(&g.field), &g.garbage.pending);
                self.plan(g);
                self.choose(rand);
                self.pad = 0;
                self.delay = self.params.delay;
            }
            _ => {}
        }
        self.pad
    }

    /// `func_80035160`. `blobs` is `D_801005FA[p] + D_80100634[p]`; the third count
    /// (`D_80100638[p]`) is only ever zeroed.
    fn switch_mode(&mut self, blobs: i32, pending: &[i16]) {
        let p = &self.params;
        let limit = i32::from(p.blobs);
        let third = 0;
        match self.mode {
            0 => {
                if blobs <= limit && third <= p.third {
                    self.mode = 1;
                }
            }
            1 => {
                // Pending garbage, summed over the pool and columns 1–5 (index 0–5).
                let waiting: i16 = pending[..6].iter().fold(0, |a, &n| a.wrapping_add(n));
                if p.flags & 1 != 0 && waiting >= 4 {
                    self.mode = 0;
                } else if blobs >= limit || third >= p.third {
                    self.mode = 2;
                }
            }
            2 => {
                if blobs <= limit {
                    self.mode = 0;
                }
            }
            _ => {}
        }
    }

    /// `func_800353A0`: clear the lists, then columns 1–6 (`func_80035468`).
    fn plan(&mut self, g: &Game) {
        for l in &mut self.lists {
            l.len = 0;
        }
        let (a, b) = g.queue[0];
        let eval = Eval { f: &g.field, a, b };
        for col in 1..=6 {
            let row = eval.top(col);
            let touches = [(col, row), (col - 1, row - 1), (col + 1, row - 1)]
                .iter()
                .any(|&(c, r)| eval.color(c, r) == a);
            let tier = match (touches, touches && eval.contact(col, row, a) == 1) {
                (true, true) => 0,
                (true, false) => 1,
                _ => 2,
            };
            let at = eval.f.at(col, row);
            for (rot, k) in [(0, eval.up(col, row - 1)), (3, eval.side(col - 1)), (1, eval.side(col + 1))] {
                let Some(k) = k else { continue };
                let list = TIERS[tier][k];
                // func_800376AC/77C8/78E4: no non-matching placement on a spawn column that is
                // already stacked above row 9.
                if list >= 3 && col == 3 && row < 9 {
                    continue;
                }
                self.lists[list].push(Target { x: at.x, y: at.y, rot });
            }
        }
    }

    /// `func_80037A00`: the first non-empty list in this mode's order.
    fn choose(&mut self, rand: &mut BiosRand) {
        let Some(order) = ORDER.get(self.mode as usize) else { return };
        let Some(list) = order.iter().map(|&i| &self.lists[i]).find(|l| l.len > 0) else {
            return;
        };
        let e = list.entries();
        self.target = match self.params.select {
            // func_8003821C
            0 => e[(rand.next() as i32 % e.len() as i32) as usize],
            // func_800382E0: the last entry if it is higher than the first, else the first.
            1 => {
                if e[e.len() - 1].y < e[0].y {
                    e[e.len() - 1]
                } else {
                    e[0]
                }
            }
            // func_800383A0: the deepest, the later one on a tie.
            2 => e.iter().fold(e[0], |best, &t| if t.y >= best.y { t } else { best }),
            _ => return,
        };
    }

    /// `func_80038490`, state 0.
    fn drive(&mut self, g: &Game, rand: &mut BiosRand) {
        if self.params.flags & 4 != 0 && g.field.at(3, 9).state == st::SETTLED {
            self.delay = 0;
        }
        if self.mode == 0 || self.delay <= 0 {
            self.act(g);
        } else {
            self.idle(rand);
        }
        if self.delay > 0 {
            self.delay -= 1;
        }
    }

    /// `func_80038598`.
    fn act(&mut self, g: &Game) {
        let p = &g.player;
        if rotation_blocked(g, 1) && rotation_blocked(g, -1) {
            return;
        }
        let t = self.target;
        if p.rot != t.rot {
            // Release first, so that every turn is a fresh press.
            if self.pad != 0 {
                self.pad = 0;
            } else {
                match t.rot {
                    1 => self.pad = pad::CIRCLE,
                    3 => self.pad = pad::CROSS,
                    0 => self.pad = 0,
                    _ => {}
                }
            }
        } else if p.x == t.x {
            self.pad = 0;
        } else if p.x < t.x {
            self.pad = pad::RIGHT;
        } else {
            self.pad = pad::LEFT;
        }
        let aligned = p.x == t.x && p.rot == t.rot;
        if self.params.flags & 2 != 0 {
            if i32::from(self.params.drop_y) < p.y && aligned {
                self.pad = pad::DOWN;
            }
        } else if aligned {
            self.pad = 0;
        }
    }

    /// `func_80038844`: every 31 frames of the delay, release or tap a random direction.
    fn idle(&mut self, rand: &mut BiosRand) {
        self.wiggle -= 1;
        if self.wiggle != -1 {
            return;
        }
        if self.pad != 0 {
            self.pad = 0;
        } else {
            self.pad = [pad::RIGHT, pad::LEFT, pad::RIGHT, pad::LEFT][(rand.next() % 4) as usize];
        }
        self.wiggle = 30;
    }
}

/// The blobs in the play area (columns 1–6 above the floor). Stands for the counters the
/// original keeps (`D_80100634` coloured + `D_801005FA` grey/special: +2 on lock, + garbage
/// dropped in, −1 when a clear finishes), which agree with the field whenever the AI plans.
pub fn field_blobs(f: &Field) -> i32 {
    let rows = f.rows() - 1;
    (1..=6).map(|c| (0..rows).filter(|&r| f.cell(c, r).state != st::EMPTY).count() as i32).sum()
}

/// `func_800332F4(dir)`: the cell the satellite would turn into is occupied (any state).
fn rotation_blocked(g: &Game, dir: i32) -> bool {
    let p = &g.player;
    let (dx, dy) = DIR[((i32::from(p.rot) + dir) & 3) as usize];
    // C division: truncates toward zero.
    let c = (p.x + dx) / 16;
    let r = (p.y + dy + 8) / 16;
    g.field.at(c, r).state != st::EMPTY
}

struct Eval<'a> {
    f: &'a Field,
    /// Pivot (`D_80069A08`) and satellite (`D_800699F0`) colours of the next pair.
    a: u8,
    b: u8,
}

impl Eval<'_> {
    fn color(&self, c: i32, r: i32) -> u8 {
        self.f.at(c, r).color
    }

    /// The first settled cell (or the floor) from row 6 down.
    fn top(&self, col: i32) -> i32 {
        let mut r = 6;
        loop {
            let cell = self.f.at(col, r);
            if cell.state == st::SETTLED || cell.color == WALL {
                return r;
            }
            r += 1;
        }
    }

    /// `func_80035964`: `color` at (c, r) would join a group of three: two of its neighbours
    /// match, or a matching neighbour has a matching neighbour of its own.
    fn three(&self, c: i32, r: i32, color: u8) -> bool {
        const N4: [(i32, i32); 4] = [(0, -1), (-1, 0), (0, 1), (1, 0)];
        let hits: Vec<(i32, i32)> =
            N4.iter().filter(|&&(dc, dr)| self.color(c + dc, r + dr) == color).copied().collect();
        hits.len() >= 2
            || hits.iter().any(|&(dc, dr)| {
                N4.iter().any(|&(ec, er)| {
                    (dc + ec, dr + er) != (0, 0) && self.color(c + dc + ec, r + dr + er) == color
                })
            })
    }

    /// `func_800356D0`: 1 if the cell at (c, r) or one of the two above its sides is `color`
    /// and forms three there.
    fn contact(&self, c: i32, r: i32, color: u8) -> i32 {
        let hit = [(c, r), (c - 1, r - 1), (c + 1, r - 1)]
            .iter()
            .any(|&(x, y)| self.color(x, y) == color && self.three(x, y, color));
        i32::from(hit)
    }

    /// `func_80036E38`: the satellite above the pivot, which rests on row `r + 1`. The third
    /// test reads row `c − 1` instead of `r − 1` (the row offset is built from the column), so
    /// it looks above the visible field.
    fn up(&self, c: i32, r: i32) -> Option<usize> {
        if self.a == self.b || self.color(c - 1, r - 1) == self.b || self.color(c + 1, c - 1) == self.b {
            Some(usize::from(self.contact(c, r, self.b) != 1))
        } else {
            Some(2)
        }
    }

    /// `func_80036F84` / `func_800371A8`: the satellite beside the pivot, in column `sc`.
    fn side(&self, sc: i32) -> Option<usize> {
        if sc == 0 || sc == 7 {
            return None;
        }
        let r = self.top(sc);
        if [(sc, r), (sc - 1, r - 1), (sc + 1, r - 1)].iter().any(|&(c, y)| self.color(c, y) == self.b) {
            Some(usize::from(self.contact(sc, r, self.b) != 1))
        } else {
            Some(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Random mid-game situations for the differential check against the original code
    /// (`CPU_DUMP=<file> cargo test -p stackers-core cpu::tests::dump -- --ignored`, then the
    /// emulator script compares `func_80034C60` on each line).
    #[test]
    #[ignore]
    fn dump() {
        use std::fmt::Write as _;
        let path = std::env::var("CPU_DUMP").expect("CPU_DUMP");
        let mut rng = BiosRand::new(std::env::var("CPU_SEED").map_or(12345, |v| v.parse().unwrap()));
        let mut r = |n: u32| rng.next() % n;
        let mut out = String::new();
        for _ in 0..20000 {
            let mut g = Game::new(1, 3);
            let colors = 3 + r(3) as u8;
            for c in 1..=6 {
                let h = r(13) as usize;
                for k in 0..h {
                    let row = 17 - k;
                    let cell = g.field.cell_mut(c, row);
                    cell.state = [st::SETTLED, st::SETTLED, st::SETTLED, st::SETTLED, st::SETTLED, st::SETTLED, st::FALLING, st::CLEARING][r(8) as usize];
                    cell.color = match r(10) { 0 => 0, 1 => 6, _ => 1 + r(colors as u32) as u8 };
                }
            }
            g.queue[0] = (1 + r(colors as u32) as u8, 1 + r(colors as u32) as u8);
            g.player.state = [ps::CONTROL, ps::CONTROL, ps::SPAWN, ps::INTRO, ps::LANDED][r(5) as usize];
            g.player.x = 16 + 16 * r(6) as i32;
            g.player.y = 72 + r(200) as i32;
            g.player.rot = r(4) as u8;
            for c in 0..7 {
                g.garbage.pending[c] = r(3) as i16;
            }
            let variant = r(2) as usize;
            let mut cpu = Cpu::new(Params::stage(variant, r(14) as usize), r(40) as i16 - 2);
            if r(3) == 0 {
                cpu.params.select = r(3) as i16;
            }
            cpu.mode = r(3) as i16;
            cpu.delay = r(40) as i16 - 5;
            cpu.pad = [0, pad::RIGHT, pad::LEFT, pad::CIRCLE][r(4) as usize];
            cpu.target = Target {
                x: 16 + 16 * r(6) as i32,
                y: r(300) as i32,
                rot: [0, 1, 3, 2][r(4) as usize],
            };
            let seed = r(0x7FFF) * 7919;
            let p = r(2);
            // Input line.
            let mut line = format!("{p} {seed} {} {} {} {} ", g.queue[0].0, g.queue[0].1, field_blobs(&g.field), g.player.state);
            write!(line, "{} {} {} ", g.player.x, g.player.y, g.player.rot).unwrap();
            for n in &g.garbage.pending {
                write!(line, "{n},").unwrap();
            }
            let pr = &cpu.params;
            write!(line, " {} {} {} {} {} {} ", pr.select, pr.flags, pr.blobs, pr.third, pr.drop_y, pr.delay).unwrap();
            let t = cpu.target;
            write!(line, "{} {} {} {} {} {} {} ", cpu.mode, cpu.delay, cpu.wiggle, cpu.pad, t.x, t.y, t.rot).unwrap();
            for c in 0..8 {
                for row in 0..g.field.rows() {
                    let cell = g.field.cell(c, row);
                    write!(line, "{}:{}:{}:{},", cell.state, cell.color, cell.x, cell.y).unwrap();
                }
            }
            // Output.
            let mut rand = BiosRand::new(seed);
            let held = cpu.step(&g, &mut rand);
            let t = cpu.target;
            write!(line, " => {} {} {} {} {} {} {} {} ", held, cpu.mode, cpu.delay, cpu.wiggle, t.x, t.y, t.rot, rand.seed).unwrap();
            for l in &cpu.lists {
                for e in l.entries() {
                    write!(line, "{}:{}:{},", e.x, e.y, e.rot).unwrap();
                }
                line.push('|');
            }
            line.push('\n');
            out.push_str(&line);
        }
        std::fs::write(path, out).unwrap();
    }
}

#[cfg(test)]
mod play {
    use super::*;
    use crate::versus::Versus;

    /// Two CPUs on the same stage play until one tops out; they should clear and chain.
    #[test]
    fn cpus_play_each_other() {
        for (seed, stage) in [(1, 0), (2, 5), (3, 13)] {
            let mut v = Versus::new(seed, [3, 3]);
            v.cpu = [Some(Cpu::new(Params::stage(0, stage), 0)), Some(Cpu::new(Params::stage(0, stage), 0))];
            let mut frames = 0;
            while v.loser().is_none() && frames < 60 * 60 * 10 {
                v.step([0, 0]);
                frames += 1;
            }
            let s: Vec<_> = v.games.iter().map(|g| (g.stats.pieces, g.stats.cleared, g.stats.max_chain)).collect();
            eprintln!("stage {stage}: {frames} frames, loser {:?}, (pieces, cleared, max chain) {s:?}", v.loser());
            assert!(s.iter().all(|&(pieces, cleared, _)| pieces > 10 && cleared > 0));
        }
    }
}
