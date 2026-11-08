//! PS1 VAB sound banks: `.VH` header (programs, tones, VAG sizes) + `.VB` body (SPU ADPCM).
//!
//! Layout (VAB version 6, as on the disc):
//! - 32-byte header: `"pBAV"`, version, id, size, …, `u16 programs` (+0x12), `u16 tones`
//!   (+0x14), `u16 vags` (+0x16), master volume/pan, attributes.
//! - 128 program records × 16 bytes (`+0` tone count, `+1` volume, `+4` pan).
//! - `programs` × 16 tone records × 32 bytes (`VagAtr`).
//! - 256 × `u16` VAG sizes in units of 8 bytes; entry 0 is unused.
//!
//! The body is the VAGs back to back. Each is a run of 16-byte ADPCM blocks.

use crate::{u16le, Error};

const HEADER: usize = 32;
const PROGRAMS: usize = 128;
const PROGRAM_SIZE: usize = 16;
const TONES_PER_PROGRAM: usize = 16;
const TONE_SIZE: usize = 32;

/// A tone (`VagAtr`): which VAG plays for a key range, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tone {
    pub priority: u8,
    /// 4 = reverb on.
    pub mode: u8,
    pub volume: u8,
    pub pan: u8,
    /// Key that plays the sample at its recorded pitch (44.1 kHz SPU rate).
    pub center: u8,
    /// Fine tune of the centre key, in 1/128 semitone.
    pub shift: u8,
    pub min: u8,
    pub max: u8,
    pub adsr1: u16,
    pub adsr2: u16,
    pub program: u16,
    /// 1-based VAG index into the body.
    pub vag: u16,
}

impl Tone {
    /// Playback rate in Hz when keyed on `note` (SPU pitch 0x1000 = 44100 Hz at the centre key).
    pub fn rate_for_note(&self, note: u8) -> f64 {
        let semis = f64::from(note) - f64::from(self.center) - f64::from(self.shift) / 128.0;
        44100.0 * (semis / 12.0).exp2()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub volume: u8,
    pub pan: u8,
    pub tones: Vec<Tone>,
}

#[derive(Clone, Debug)]
pub struct VabHeader {
    pub volume: u8,
    pub pan: u8,
    /// Indexed by program number (only the first `programs` slots have tone records).
    pub programs: Vec<Program>,
    /// Byte ranges of each VAG in the body; index 0 = VAG 1.
    pub vags: Vec<std::ops::Range<usize>>,
}

impl VabHeader {
    pub fn parse(vh: &[u8]) -> Result<VabHeader, Error> {
        if vh.get(0..4) != Some(b"pBAV") {
            return Err(Error::BadMagic("VAB"));
        }
        let n_prog = usize::from(u16le(vh, 0x12)?);
        let n_vag = usize::from(u16le(vh, 0x16)?);
        if n_prog > PROGRAMS || n_vag > 255 {
            return Err(Error::Unsupported("VAB counts"));
        }
        let tones_at = HEADER + PROGRAMS * PROGRAM_SIZE;
        let sizes_at = tones_at + n_prog * TONES_PER_PROGRAM * TONE_SIZE;
        if vh.len() < sizes_at + 512 {
            return Err(Error::Truncated);
        }
        let mut programs = Vec::with_capacity(n_prog);
        for p in 0..n_prog {
            let rec = &vh[HEADER + p * PROGRAM_SIZE..][..PROGRAM_SIZE];
            let count = usize::from(rec[0]).min(TONES_PER_PROGRAM);
            let tones = (0..count)
                .map(|t| {
                    let a = &vh[tones_at + (p * TONES_PER_PROGRAM + t) * TONE_SIZE..][..TONE_SIZE];
                    let h = |o: usize| u16::from_le_bytes([a[o], a[o + 1]]);
                    Tone {
                        priority: a[0],
                        mode: a[1],
                        volume: a[2],
                        pan: a[3],
                        center: a[4],
                        shift: a[5],
                        min: a[6],
                        max: a[7],
                        adsr1: h(0x10),
                        adsr2: h(0x12),
                        program: h(0x14),
                        vag: h(0x16),
                    }
                })
                .collect();
            programs.push(Program { volume: rec[1], pan: rec[4], tones });
        }
        let mut vags = Vec::with_capacity(n_vag);
        let mut at = 0;
        for i in 1..=n_vag {
            let len = usize::from(u16le(vh, sizes_at + 2 * i)?) * 8;
            vags.push(at..at + len);
            at += len;
        }
        Ok(VabHeader { volume: vh[0x18], pan: vh[0x19], programs, vags })
    }

    /// Decodes VAG `index` (1-based, as in [`Tone::vag`]) from the body.
    pub fn vag(&self, body: &[u8], index: u16) -> Result<Sample, Error> {
        let r = self
            .vags
            .get(usize::from(index).wrapping_sub(1))
            .ok_or(Error::NotFound)?;
        decode_adpcm(body.get(r.clone()).ok_or(Error::Truncated)?)
    }
}

/// Decoded mono PCM. Plays at 44.1 kHz on the centre key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sample {
    pub pcm: Vec<i16>,
    /// Sample index where the loop restarts, if the data loops.
    pub loop_start: Option<usize>,
}

const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

/// SPU ADPCM: 16-byte blocks of 28 samples. Byte 0 = filter << 4 | shift, byte 1 = flags
/// (bit 0 end, bit 1 repeat, bit 2 loop start). Decoding stops after the first block with the
/// end flag.
pub fn decode_adpcm(data: &[u8]) -> Result<Sample, Error> {
    let mut out = Sample::default();
    let (mut s1, mut s2) = (0i32, 0i32);
    let mut loop_mark = None;
    for block in data.chunks_exact(16) {
        let shift = u32::from(block[0] & 0x0F);
        let filter = usize::from(block[0] >> 4);
        let flags = block[1];
        let (f0, f1) = *FILTERS.get(filter).ok_or(Error::Unsupported("ADPCM filter"))?;
        if flags & 4 != 0 {
            loop_mark = Some(out.pcm.len());
        }
        let shift = if shift > 12 { 9 } else { shift };
        for &b in &block[2..] {
            for nib in [b & 0x0F, b >> 4] {
                let raw = i32::from(((nib as i8) << 4) >> 4); // sign-extend 4 bits
                let s = ((raw << 12) >> shift) + ((s1 * f0 + s2 * f1 + 32) >> 6);
                let s = s.clamp(-0x8000, 0x7FFF);
                out.pcm.push(s as i16);
                s2 = s1;
                s1 = s;
            }
        }
        if flags & 1 != 0 {
            if flags & 2 != 0 {
                out.loop_start = loop_mark;
            }
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_filtered_block_and_stops_at_end() {
        // Block 1: filter 0, shift 12, nibbles 1 then -1, loop start.
        // Block 2: filter 1, shift 12, all zero nibbles, end + repeat.
        // Block 3 must be ignored.
        let mut d = vec![0u8; 48];
        d[0] = 0x0C;
        d[1] = 4;
        d[2] = 0xF1;
        d[16] = 0x1C;
        d[17] = 3;
        d[32] = 0x0C;
        d[34] = 0x77;
        let s = decode_adpcm(&d).unwrap();
        assert_eq!(s.pcm.len(), 56);
        assert_eq!(&s.pcm[..3], &[1, -1, 0]);
        assert_eq!(s.loop_start, Some(0));
        // Filter 1 decays the last value: 0 + (0·60 + 32) >> 6 = 0.
        assert!(s.pcm[28..].iter().all(|&v| v == 0));
    }

    #[test]
    fn centre_key_is_44k() {
        let t = Tone {
            priority: 0, mode: 0, volume: 127, pan: 64, center: 60, shift: 0, min: 0, max: 127,
            adsr1: 0, adsr2: 0, program: 0, vag: 1,
        };
        assert_eq!(t.rate_for_note(60), 44100.0);
        assert!((t.rate_for_note(48) - 22050.0).abs() < 1e-6);
    }
}
