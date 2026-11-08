//! Sound effects for the page: effect id → decoded PCM, via the executable's tables.

use std::collections::HashMap;

use stackers_formats::{
    sound::SoundTables,
    vab::{Sample, VabHeader},
};
use wasm_bindgen::prelude::*;

use crate::js_err;

struct LoadedBank {
    header: VabHeader,
    body: Vec<u8>,
}

#[wasm_bindgen]
pub struct Sfx {
    tables: SoundTables,
    banks: HashMap<String, LoadedBank>,
}

/// One effect ready for Web Audio: mono PCM in −1…1 at `rate` Hz.
#[wasm_bindgen]
pub struct SfxSample {
    rate: f32,
    gain: f32,
    pcm: Vec<f32>,
    loop_start: i32,
}

#[wasm_bindgen]
impl SfxSample {
    #[wasm_bindgen(getter)]
    pub fn rate(&self) -> f32 {
        self.rate
    }
    /// Tone × program volume, 0…1.
    #[wasm_bindgen(getter)]
    pub fn gain(&self) -> f32 {
        self.gain
    }
    /// Sample index the loop restarts at, or −1 for one-shot.
    #[wasm_bindgen(getter)]
    pub fn loop_start(&self) -> i32 {
        self.loop_start
    }
    pub fn pcm(&self) -> Vec<f32> {
        self.pcm.clone()
    }
}

#[wasm_bindgen]
impl Sfx {
    /// `exe` is `SLUS_003.15` from the user's disc.
    #[wasm_bindgen(constructor)]
    pub fn new(exe: &[u8]) -> Result<Sfx, JsError> {
        Ok(Sfx { tables: SoundTables::from_exe(exe).map_err(js_err)?, banks: HashMap::new() })
    }

    /// Disc paths `[vh, vb]` of the VAB that effect `id` lives in (empty if none).
    pub fn bank_paths(&self, id: u32) -> Vec<String> {
        self.bank_for(id)
            .and_then(|b| b.vab_paths())
            .map(|(vh, vb)| vec![vh, vb])
            .unwrap_or_default()
    }

    /// Register a VAB (`vh_path` as returned by [`Sfx::bank_paths`]).
    pub fn add_bank(&mut self, vh_path: &str, vh: &[u8], vb: Vec<u8>) -> Result<(), JsError> {
        let header = VabHeader::parse(vh).map_err(js_err)?;
        self.banks.insert(vh_path.to_string(), LoadedBank { header, body: vb });
        Ok(())
    }

    /// Decode effect `id`; `None` if its bank isn't loaded or it names no tone.
    pub fn sample(&self, id: u32) -> Option<SfxSample> {
        let e = *self.tables.effects.get(id as usize)?;
        let (vh_path, _) = self.bank_for(id)?.vab_paths()?;
        let bank = self.banks.get(&vh_path)?;
        let prog = bank.header.programs.get(usize::from(e.program))?;
        let tone = prog.tones.iter().find(|t| (t.min..=t.max).contains(&e.key))?;
        let Sample { pcm, loop_start } = bank.header.vag(&bank.body, tone.vag).ok()?;
        Some(SfxSample {
            rate: tone.rate_for_note(e.key) as f32,
            gain: f32::from(tone.volume) / 127.0 * f32::from(prog.volume) / 127.0,
            pcm: pcm.iter().map(|&s| f32::from(s) / 32768.0).collect(),
            loop_start: loop_start.map_or(-1, |s| s as i32),
        })
    }
}

impl Sfx {
    fn bank_for(&self, id: u32) -> Option<&stackers_formats::sound::Bank> {
        let e = self.tables.effects.get(id as usize)?;
        self.tables.banks.get(usize::from(e.bank))
    }
}
