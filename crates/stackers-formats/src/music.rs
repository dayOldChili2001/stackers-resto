//! The CD-DA music table inside the executable (`D_8010062C` → `0x800134E0`, set up by
//! `func_80040394`), played by the CD state machine `func_8003F98C`.
//!
//! 11 entries of 12 bytes: `u8 track` (index into the TOC positions `D_800C2ED0`), 3 unused
//! bytes, `i32 sectors` (how far playback runs before the end check fires), `i32 flag`
//! (−1 = play once and stop, 0 = seek back to the track start and play again).
//! Started by `func_80040AE8(index, …)`.

use crate::{exe::Exe, u32le, Error};

const TABLE: u32 = 0x8001_34E0;
/// The table runs up to the next one (`0x80013564`).
const COUNT: usize = (0x8001_3564 - 0x8001_34E0) / 12;

/// Bytes per raw CD-DA sector (588 stereo 16-bit frames at 44.1 kHz).
pub const SECTOR: usize = 2352;
/// Each audio track's `.bin` starts with a 2-second pregap (`INDEX 00` → `INDEX 01`).
pub const PREGAP_SECTORS: usize = 150;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Track {
    /// Disc track number (2–13).
    pub number: u8,
    /// Sectors played from `INDEX 01`.
    pub sectors: u32,
    /// `true` = loops back to the start; `false` = plays once.
    pub loops: bool,
}

impl Track {
    /// Byte range of the played audio inside this track's raw `.bin` file.
    pub fn byte_range(&self) -> std::ops::Range<usize> {
        let start = PREGAP_SECTORS * SECTOR;
        start..start + self.sectors as usize * SECTOR
    }
}

pub fn tracks_from_exe(exe: &[u8]) -> Result<Vec<Track>, Error> {
    let raw = Exe::parse(exe)?.at(TABLE, COUNT * 12)?;
    raw.chunks_exact(12)
        .map(|e| {
            let sectors = u32le(e, 4)?;
            let flag = u32le(e, 8)?;
            if !(1..=99).contains(&e[0]) || sectors == 0 || (flag != 0 && flag != u32::MAX) {
                return Err(Error::Unsupported("CD-DA table entry"));
            }
            Ok(Track { number: e[0], sectors, loops: flag == 0 })
        })
        .collect()
}
