//! The sound-effect tables inside the executable (`SLUS_003.15`), set up by `func_80040394`
//! (`D_80066200`, `D_80100650`) and used by `func_800407C4`.
//!
//! - Banks at `0x80012A94`, 0x44 bytes each: 8-byte name (space padded), `u32` kind
//!   (0 = `SOUND/VAB_SE`, 1 = `SOUND/VAB_CHR`, 2 = XA), then driver state.
//! - Effects at `0x80013160`, 4 bytes each: bank slot, program, key, unused. Played with
//!   `SsVoKeyOn((vab << 8) | program, key << 8, …)`.

use crate::{u32le, Error};

const BANKS: u32 = 0x8001_2A94;
const BANK_SIZE: usize = 0x44;
const BANK_COUNT: usize = 24;
const EFFECTS: u32 = 0x8001_3160;
/// The effect table runs up to the next table (`0x80013468`).
const EFFECT_COUNT: usize = (0x8001_3468 - 0x8001_3160) / 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bank {
    pub name: String,
    pub kind: u32,
}

impl Bank {
    /// Disc paths of the `.VH` / `.VB` pair, if this bank is a VAB.
    pub fn vab_paths(&self) -> Option<(String, String)> {
        let dir = match self.kind {
            0 => "VAB_SE",
            1 => "VAB_CHR",
            _ => return None,
        };
        let base = format!("DERODERO/SOUND/{dir}/{}", self.name);
        Some((format!("{base}.VH"), format!("{base}.VB")))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub bank: u8,
    pub program: u8,
    pub key: u8,
}

#[derive(Clone, Debug)]
pub struct SoundTables {
    pub banks: Vec<Bank>,
    pub effects: Vec<Effect>,
}

/// Maps RAM addresses to offsets in a PS-X EXE.
struct Exe<'a> {
    data: &'a [u8],
    text: u32,
}

impl<'a> Exe<'a> {
    fn parse(data: &'a [u8]) -> Result<Self, Error> {
        if data.get(0..8) != Some(b"PS-X EXE") {
            return Err(Error::BadMagic("PS-X EXE"));
        }
        Ok(Exe { data, text: u32le(data, 0x18)? })
    }

    fn at(&self, addr: u32, len: usize) -> Result<&'a [u8], Error> {
        let off = addr.checked_sub(self.text).ok_or(Error::Truncated)? as usize + 0x800;
        self.data.get(off..off + len).ok_or(Error::Truncated)
    }
}

impl SoundTables {
    pub fn from_exe(exe: &[u8]) -> Result<SoundTables, Error> {
        let exe = Exe::parse(exe)?;
        let banks = (0..BANK_COUNT)
            .map(|i| {
                let rec = exe.at(BANKS + (i * BANK_SIZE) as u32, 12)?;
                let name = String::from_utf8_lossy(&rec[..8]).trim_end_matches([' ', '\0']).to_string();
                Ok(Bank { name, kind: u32le(rec, 8)? })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let raw = exe.at(EFFECTS, EFFECT_COUNT * 4)?;
        let effects = raw
            .chunks_exact(4)
            .map(|e| Effect { bank: e[0], program: e[1], key: e[2] })
            .collect();
        Ok(SoundTables { banks, effects })
    }
}
