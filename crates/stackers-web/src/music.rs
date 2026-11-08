//! CD-DA music for the page: which disc track to play and which bytes of its `.bin` hold it.

use stackers_core::tables::music_for_level;
use stackers_formats::music::{tracks_from_exe, Track};
use wasm_bindgen::prelude::*;

use crate::js_err;

#[wasm_bindgen]
pub struct Music {
    tracks: Vec<Track>,
}

#[wasm_bindgen]
impl Music {
    /// `exe` is `SLUS_003.15` from the user's disc.
    #[wasm_bindgen(constructor)]
    pub fn new(exe: &[u8]) -> Result<Music, JsError> {
        Ok(Music { tracks: tracks_from_exe(exe).map_err(js_err)? })
    }

    /// `[track_number, start_byte, end_byte, loops]` for single-player level `level` (0-based
    /// row of the level table). The bytes are offsets into that track's raw `.bin`: 16-bit
    /// little-endian stereo PCM at 44.1 kHz.
    pub fn for_level(&self, level: u32) -> Vec<u32> {
        self.entry(music_for_level(level as usize))
    }

    /// Same as [`Music::for_level`] for a raw table index (empty if out of range).
    pub fn entry(&self, index: usize) -> Vec<u32> {
        self.tracks
            .get(index)
            .map(|t| {
                let r = t.byte_range();
                vec![u32::from(t.number), r.start as u32, r.end as u32, u32::from(t.loops)]
            })
            .unwrap_or_default()
    }
}
