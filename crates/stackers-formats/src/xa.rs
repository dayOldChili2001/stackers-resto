//! XA-ADPCM music: the table inside the executable (`D_80100654` → `0x80013468`) and a decoder
//! for the interleaved `DERODERO/SOUND/XA/SND_XA0n.STR` files.
//!
//! 10 entries of 12 bytes: `u8 file` (n of `SND_XA0n`), `u8 channel` (subheader channel the CD
//! filter selects, file number always 1), 2 unused bytes, `i32 sectors`, `i32 flag`. The CD state
//! machine `func_8003F98C` (states 1–11) seeks to the file's first sector, reads at single speed
//! with the XA filter on (setmode 0x48), and polls GetlocP until it is `sectors` past the start:
//! then flag −1 pauses (plays once), 0 seeks back to the start (loops). Started by
//! `func_80040A7C(index, 1)`, stopped by `func_80040A7C(_, 2)`.
//!
//! Every audio sector on the disc is stereo, 37.8 kHz, 4-bit; each channel takes every 4th
//! sector. At single speed (75 sectors/s) one file sector lasts 504 sample frames, and one audio
//! sector holds 2016 frames: the four sectors it spans.

use crate::{exe::Exe, iso::Disc, u32le, Error};

const TABLE: u32 = 0x8001_3468;
/// The table runs up to the CD-DA one (`0x800134E0`).
const COUNT: usize = (0x8001_34E0 - 0x8001_3468) / 12;

const RAW: usize = 2352;
/// Output rate (Hz).
pub const RATE: u32 = 37_800;
/// Sample frames per file sector at single speed (37800 / 75).
pub const FRAMES_PER_SECTOR: usize = 504;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// `n` of `SND_XA0n.STR`.
    pub file: u8,
    /// Subheader channel.
    pub channel: u8,
    /// File sectors played from the start before the end check fires.
    pub sectors: u32,
    /// `true` = seeks back to the start and plays again; `false` = plays once.
    pub loops: bool,
}

impl Entry {
    pub fn path(&self) -> String {
        format!("DERODERO/SOUND/XA/SND_XA{:02}.STR", self.file)
    }
}

pub fn table_from_exe(exe: &[u8]) -> Result<Vec<Entry>, Error> {
    let raw = Exe::parse(exe)?.at(TABLE, COUNT * 12)?;
    raw.chunks_exact(12)
        .map(|e| {
            let sectors = u32le(e, 4)?;
            let flag = u32le(e, 8)?;
            if e[0] > 9 || e[1] > 31 || sectors == 0 || (flag != 0 && flag != u32::MAX) {
                return Err(Error::Unsupported("XA table entry"));
            }
            Ok(Entry { file: e[0], channel: e[1], sectors, loops: flag == 0 })
        })
        .collect()
}

/// Decodes `entry` from the disc: interleaved stereo `i16` at [`RATE`], `entry.sectors` file
/// sectors long (one loop period), each audio sector placed at its read time from the first one.
pub fn decode_entry(disc: &Disc, entry: &Entry) -> Result<Vec<i16>, Error> {
    let file = disc.find(&entry.path()).ok_or(Error::NotFound)?;
    decode(disc.raw_sectors(file)?, entry.channel, entry.sectors as usize)
}

/// Decodes channel `channel` of raw XA sectors `raw` (2352 bytes each), reading `sectors` of them.
pub fn decode(raw: &[u8], channel: u8, sectors: usize) -> Result<Vec<i16>, Error> {
    let sectors = sectors.min(raw.len() / RAW);
    let mut out = vec![0i16; sectors * FRAMES_PER_SECTOR * 2];
    let mut state = [Channel::default(); 2];
    let mut first = None;
    for i in 0..sectors {
        let s = &raw[i * RAW..(i + 1) * RAW];
        let (ch, submode, coding) = (s[17], s[18], s[19]);
        // Form 2 audio (submode bits 5 and 2) on our channel; null filler sectors are skipped.
        if ch != channel || submode & 0x24 != 0x24 {
            continue;
        }
        if coding != 0x01 {
            return Err(Error::Unsupported("XA coding other than stereo 37.8 kHz 4-bit"));
        }
        let start = (i - *first.get_or_insert(i)) * FRAMES_PER_SECTOR * 2;
        let Some(dst) = out.get_mut(start..) else { break };
        decode_sector(&s[24..24 + 18 * 128], &mut state, dst);
    }
    Ok(out)
}

#[derive(Clone, Copy, Default)]
struct Channel {
    old: i32,
    older: i32,
}

const K0: [i32; 4] = [0, 60, 115, 98];
const K1: [i32; 4] = [0, 0, -52, -55];

/// One stereo 4-bit sector: 18 groups of 128 bytes, each 8 sound units of 28 samples
/// (even units left, odd right). Writes up to 2016 frames into `dst`.
fn decode_sector(data: &[u8], state: &mut [Channel; 2], dst: &mut [i16]) {
    for (g, group) in data.chunks_exact(128).enumerate() {
        for unit in 0..8 {
            let side = unit & 1;
            let param = group[4 + unit];
            let shift = match param & 0x0F {
                s @ 0..=12 => s,
                _ => 9,
            };
            let filter = usize::from((param >> 4) & 3);
            let st = &mut state[side];
            for j in 0..28 {
                let byte = group[16 + j * 4 + unit / 2];
                let nibble = if unit % 2 == 0 { byte & 0x0F } else { byte >> 4 };
                let t = (i32::from((u16::from(nibble) << 12) as i16)) >> shift;
                let s = (t + ((st.old * K0[filter] + st.older * K1[filter] + 32) >> 6)).clamp(-0x8000, 0x7FFF);
                st.older = st.old;
                st.old = s;
                let frame = g * 112 + (unit / 2) * 28 + j;
                if let Some(d) = dst.get_mut(frame * 2 + side) {
                    *d = s as i16;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_stays_silent() {
        let mut raw = vec![0u8; RAW * 4];
        for i in 0..4 {
            raw[i * RAW + 17] = 0;
            raw[i * RAW + 18] = 0x64;
            raw[i * RAW + 19] = 0x01;
        }
        let pcm = decode(&raw, 0, 4).unwrap();
        assert_eq!(pcm.len(), 4 * FRAMES_PER_SECTOR * 2);
        assert!(pcm.iter().all(|&s| s == 0));
    }

    #[test]
    fn nibble_scaling() {
        // One sector, unit 0 (left) shift 12, filter 0, first sample nibble 7 → 7; unit 1 (right)
        // nibble 8 → −8 at shift 12.
        let mut raw = vec![0u8; RAW];
        raw[17] = 2;
        raw[18] = 0x64;
        raw[19] = 0x01;
        raw[24 + 4] = 12;
        raw[24 + 5] = 12;
        raw[24 + 16] = 0x87;
        let pcm = decode(&raw, 2, 1).unwrap();
        assert_eq!(&pcm[..2], &[7, -8]);
    }
}
