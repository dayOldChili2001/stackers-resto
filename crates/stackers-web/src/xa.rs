//! XA-ADPCM music for the page: decodes a table entry from the data track to a WAV file.

use stackers_formats::xa;
use wasm_bindgen::prelude::*;

use crate::js_err;

#[wasm_bindgen]
pub struct Xa {
    table: Vec<xa::Entry>,
}

#[wasm_bindgen]
impl Xa {
    /// `exe` is `SLUS_003.15` from the user's disc.
    #[wasm_bindgen(constructor)]
    pub fn new(exe: &[u8]) -> Result<Xa, JsError> {
        Ok(Xa { table: xa::table_from_exe(exe).map_err(js_err)? })
    }

    /// Whether entry `index` loops (seeks back to its start at the end).
    pub fn loops(&self, index: usize) -> bool {
        self.table.get(index).is_some_and(|e| e.loops)
    }

    /// The disc path of the file entry `index` plays (as [`crate::GameDisc::raw_file`] takes it).
    pub fn path(&self, index: usize) -> Option<String> {
        self.table.get(index).map(xa::Entry::path)
    }

    /// Entry `index` as a 16-bit stereo WAV at 37.8 kHz, one loop period long. `file` is its
    /// file's raw sectors ([`Xa::path`] through [`crate::GameDisc::raw_file`]).
    pub fn wav(&self, file: &[u8], index: usize) -> Result<Vec<u8>, JsError> {
        let e = self.table.get(index).ok_or_else(|| JsError::new("no such XA entry"))?;
        let pcm = xa::decode(file, e.channel, e.sectors as usize).map_err(js_err)?;
        let data = pcm.len() as u32 * 2;
        let mut w = Vec::with_capacity(44 + data as usize);
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&xa::RATE.to_le_bytes());
        w.extend_from_slice(&(xa::RATE * 4).to_le_bytes());
        w.extend_from_slice(&4u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data.to_le_bytes());
        for s in pcm {
            w.extend_from_slice(&s.to_le_bytes());
        }
        Ok(w)
    }
}
