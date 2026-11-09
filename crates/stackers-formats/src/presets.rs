//! `DATA/FIL2_DAT.OBJ`: preset field layouts, loaded to RAM `0x80170000` (file table index 0)
//! and read by `func_80027098(set, n)`.
//!
//! The file starts with one pointer per set (RAM addresses); each set is a table of pointers
//! to layouts, and the set tables run back to back up to the first layout. A layout is one
//! `i16` per cell, rows 6…H−2 (top to bottom), columns 1–6: `0x80` = empty, else a colour.
//! Set 0 (12 layouts of 72 rows) is Chain Reaction's (secret, mode 7, H = 79); set 1 holds the
//! five Time Trial courses (12 rows, H = 19), in course order.

use crate::{u16le, u32le, Error};

const BASE: u32 = 0x8017_0000;

/// Set of the Time Trial courses.
pub const TIME_TRIAL: usize = 1;
/// Cells in a Time Trial layout (rows 6…17 × 6 columns).
pub const TIME_TRIAL_CELLS: usize = 12 * 6;

#[derive(Clone, Debug)]
pub struct Presets {
    data: Vec<u8>,
    /// File offsets of each set's layouts.
    sets: Vec<Vec<usize>>,
}

impl Presets {
    pub fn parse(data: &[u8]) -> Result<Presets, Error> {
        let off = |p: u32| -> Result<usize, Error> {
            p.checked_sub(BASE).map(|o| o as usize).filter(|&o| o < data.len()).ok_or(Error::BadMagic("FIL2_DAT pointer"))
        };
        let first_table = off(u32le(data, 0)?)?;
        if first_table == 0 || first_table % 4 != 0 {
            return Err(Error::BadMagic("FIL2_DAT header"));
        }
        let tables: Vec<usize> =
            (0..first_table / 4).map(|i| u32le(data, 4 * i).and_then(off)).collect::<Result<_, _>>()?;
        let mut layouts_start = data.len();
        let mut sets = Vec::new();
        for (i, &t) in tables.iter().enumerate() {
            let end = tables.get(i + 1).copied().unwrap_or(usize::MAX);
            let mut set = Vec::new();
            let mut at = t;
            while at < end && at < layouts_start {
                let l = off(u32le(data, at)?)?;
                layouts_start = layouts_start.min(l);
                set.push(l);
                at += 4;
            }
            sets.push(set);
        }
        Ok(Presets { data: data.to_vec(), sets })
    }

    pub fn sets(&self) -> usize {
        self.sets.len()
    }

    pub fn len(&self, set: usize) -> usize {
        self.sets.get(set).map_or(0, Vec::len)
    }

    /// Layout `n` of `set`, `cells` values long (`(H − 7) · 6`).
    pub fn layout(&self, set: usize, n: usize, cells: usize) -> Result<Vec<i16>, Error> {
        let at = *self.sets.get(set).and_then(|s| s.get(n)).ok_or(Error::NotFound)?;
        (0..cells).map(|i| u16le(&self.data, at + 2 * i).map(|v| v as i16)).collect()
    }

    /// The five Time Trial courses.
    pub fn time_trial(&self) -> Result<Vec<Vec<i16>>, Error> {
        (0..self.len(TIME_TRIAL)).map(|n| self.layout(TIME_TRIAL, n, TIME_TRIAL_CELLS)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sets_and_layouts() {
        // Two sets: set 0 with one layout, set 1 with two; layouts of 2 cells.
        let mut d = Vec::new();
        for p in [0x8017_0008u32, 0x8017_000C, 0x8017_0014, 0x8017_0018, 0x8017_001C] {
            d.extend_from_slice(&p.to_le_bytes());
        }
        for v in [0x80i16, 1, 2, 3, 4, 0x80] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        let p = Presets::parse(&d).unwrap();
        assert_eq!(p.sets(), 2);
        assert_eq!((p.len(0), p.len(1)), (1, 2));
        assert_eq!(p.layout(0, 0, 2).unwrap(), vec![0x80, 1]);
        assert_eq!(p.layout(1, 1, 2).unwrap(), vec![4, 0x80]);
    }
}
