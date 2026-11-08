//! `DATA/BLT_ANM.OBJ`: blob animation scripts and sprite frames.
//!
//! The file is loaded at RAM `0x801C0000` and is all absolute pointers into itself.
//!
//! - `+0`: piece table, `[colour][player_state] -> script` (`func_8001F9F8`).
//! - `+4`: cell table (inline, one pointer per colour), `[colour][cell_state] -> script` (`func_80021E18`). For some states the
//!   entry is a further table indexed by the neighbour mask, and then by the arm bits.
//! - A script is a list of `(u32 duration, u32 frame_ptr)` pairs. Duration `0xFFFF` is a command:
//!   `1` = raise "done" (later entries are ignored), `0` = jump to the next word, others = no-op.
//! - A frame is 9 × i16: `u, v, w, h, dx, dy, page, clut, flip` (`func_8001B148`). `page` is the
//!   index into `DERO/DEROn.TIM` (file ids `0x47 + n`), `clut` the palette row of that sheet.

use crate::{u32le, Error};

pub const BASE: u32 = 0x801C_0000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub u: i16,
    pub v: i16,
    pub w: i16,
    pub h: i16,
    pub dx: i16,
    pub dy: i16,
    pub page: i16,
    pub clut: i16,
    pub flip: i16,
}

/// Lookup key for a cell's script (`func_80021E18`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellKey {
    pub color: u8,
    pub state: u8,
    /// Same-colour neighbour mask `+0x34`: 8 up, 4 right, 2 left, 1 down.
    pub mask: u8,
    /// Arm bits `+0x2C`: 1 left, 2 right.
    pub arms: u8,
}

#[derive(Clone, Debug)]
pub struct AnimBank {
    data: Vec<u8>,
}

impl AnimBank {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let bank = AnimBank { data: data.to_vec() };
        // The piece table and the first cell-table entry must point inside the file.
        for slot in [0, 4] {
            let p = bank.word(BASE + slot)?;
            bank.offset(p)?;
        }
        Ok(bank)
    }

    fn offset(&self, ptr: u32) -> Result<usize, Error> {
        let off = ptr.wrapping_sub(BASE) as usize;
        if off + 4 <= self.data.len() {
            Ok(off)
        } else {
            Err(Error::Truncated)
        }
    }

    fn word(&self, ptr: u32) -> Result<u32, Error> {
        u32le(&self.data, self.offset(ptr)?)
    }

    fn index(&self, table: u32, i: u32) -> Option<u32> {
        self.word(table + 4 * i).ok()
    }

    /// Script for a blob in the field (`func_80021E18`).
    pub fn cell_script(&self, key: CellKey) -> Option<u32> {
        let by_state = self.index(BASE + 4, u32::from(key.color))?;
        let entry = self.index(by_state, u32::from(key.state))?;
        if key.color == 0 || key.color == 6 {
            return Some(entry);
        }
        // Jump table 0x8001056C, indexed by state - 2.
        match key.state {
            2 | 4 => self.index(entry, u32::from(key.mask)),
            5 | 8 | 9 | 10 => {
                let t = self.index(entry, u32::from(key.mask))?;
                self.index(t, u32::from(key.arms))
            }
            _ => Some(entry),
        }
    }

    /// Script for half of the controlled pair (`func_8001F9F8`).
    pub fn piece_script(&self, color: u8, player_state: u8) -> Option<u32> {
        let colors = self.word(BASE).ok()?;
        let by_state = self.index(colors, u32::from(color))?;
        self.index(by_state, u32::from(player_state))
    }

    pub fn frame(&self, ptr: u32) -> Option<Frame> {
        let off = self.offset(ptr).ok()?;
        let b = self.data.get(off..off + 18)?;
        let h = |i: usize| i16::from_le_bytes([b[2 * i], b[2 * i + 1]]);
        Some(Frame {
            u: h(0),
            v: h(1),
            w: h(2),
            h: h(3),
            dx: h(4),
            dy: h(5),
            page: h(6),
            clut: h(7),
            flip: h(8),
        })
    }
}

/// The script interpreter (`func_80021D44` for cells, `func_8001F95C` for the pair).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScriptPlayer {
    script: u32,
    timer: i16,
    /// Frames spent in the current entry (`+0x22`).
    pub elapsed: u16,
    pub done: bool,
    pub frame: u32,
}

impl ScriptPlayer {
    pub fn new(script: u32) -> Self {
        ScriptPlayer { script, ..Default::default() }
    }

    /// `func_80021ED8`: switch to another script, replaying the frames spent in the current entry.
    pub fn restart_keep_elapsed(&mut self, bank: &AnimBank, script: u32) {
        let replay = self.elapsed;
        *self = ScriptPlayer::new(script);
        for _ in 0..replay {
            self.step(bank);
        }
    }

    pub fn step(&mut self, bank: &AnimBank) {
        if self.timer == 0xFF {
            return;
        }
        if self.timer == 0 {
            let mut p = self.script;
            // Bounded so a malformed jump loop cannot hang the page.
            for _ in 0..4096 {
                let (Ok(a), Ok(b)) = (bank.word(p), bank.word(p + 4)) else {
                    self.timer = 0xFF;
                    return;
                };
                p += 8;
                if a == 0xFFFF {
                    match b {
                        1 => self.done = true,
                        0 => match bank.word(p) {
                            Ok(target) => p = target,
                            Err(_) => {
                                self.timer = 0xFF;
                                return;
                            }
                        },
                        _ => {}
                    }
                    continue;
                }
                if !self.done {
                    self.timer = a as i16;
                    self.frame = b;
                    self.script = p;
                    self.elapsed = 0;
                }
                break;
            }
        }
        self.timer = self.timer.wrapping_sub(1);
        self.elapsed = self.elapsed.wrapping_add(1);
    }

    /// Steps until `done` is raised; `None` if it never is within `limit`.
    pub fn length(bank: &AnimBank, script: u32, limit: usize) -> Option<usize> {
        let mut p = ScriptPlayer::new(script);
        for n in 0..limit {
            if p.done {
                return Some(n);
            }
            p.step(bank);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny hand-built bank: one colour, one state, a two-frame looping script.
    fn bank() -> AnimBank {
        let mut words: Vec<u32> = vec![0; 64];
        let at = |i: usize| BASE + 4 * i as u32;
        words[0] = at(8); // piece table
        words[1] = at(10); // cell table, colour 0 (inline)
        words[2] = at(10); // cell table, colour 1
        words[8] = at(10); // piece table: colour 0 -> state table
        words[9] = at(10); // piece table: colour 1 -> state table
        words[10] = at(12); // state 0 -> script
        words[11] = at(12); // state 1 -> script
        // script at word 12: (2, f0), (3, f1), jump back to 12
        words[12] = 2;
        words[13] = at(30);
        words[14] = 3;
        words[15] = at(40);
        words[16] = 0xFFFF;
        words[17] = 0;
        words[18] = at(12);
        // done script at word 20: (1, f0), done, (5, f1)
        words[20] = 1;
        words[21] = at(30);
        words[22] = 0xFFFF;
        words[23] = 1;
        words[24] = 5;
        words[25] = at(40);
        let mut data: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        data.resize(400, 0);
        AnimBank::parse(&data).unwrap()
    }

    #[test]
    fn loops_through_jump() {
        let b = bank();
        let s = b.cell_script(CellKey { color: 1, state: 1, ..Default::default() }).unwrap();
        let mut p = ScriptPlayer::new(s);
        let mut seen = vec![];
        for _ in 0..7 {
            p.step(&b);
            seen.push((p.frame - BASE) / 4);
        }
        assert_eq!(seen, [30, 30, 40, 40, 40, 30, 30]);
        assert!(!p.done);
    }

    #[test]
    fn done_marker_freezes_frame() {
        let b = bank();
        let s = BASE + 4 * 20;
        assert_eq!(ScriptPlayer::length(&b, s, 100), Some(2));
        let mut p = ScriptPlayer::new(s);
        for _ in 0..10 {
            p.step(&b);
        }
        assert!(p.done);
        assert_eq!((p.frame - BASE) / 4, 30, "entries after the done marker are ignored");
    }
}
