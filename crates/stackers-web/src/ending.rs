//! The ending (mode 16) exposed to JavaScript: see `stackers_core::ending`. Drawn by
//! `Screen::ending`.

use stackers_core::ending::{self, Art};
use stackers_formats::sdchr::{SdBank, SdStage};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Ending {
    e: ending::Ending,
    sd: Option<SdBank>,
    stage: SdStage,
    sounds: Vec<u8>,
}

impl Ending {
    pub(crate) fn inner(&self) -> &ending::Ending {
        &self.e
    }

    pub(crate) fn stage(&self) -> &SdStage {
        &self.stage
    }
}

#[wasm_bindgen]
impl Ending {
    /// After clearing vs CPU course `course` (0–2) whose last stage was `last_stage`
    /// (`D_800C2FE0`: 14 unless a continue cut the course short).
    #[wasm_bindgen(constructor)]
    pub fn new(course: u8, last_stage: u8) -> Ending {
        Ending { e: ending::Ending::new(course, last_stage), sd: None, stage: SdStage::default(), sounds: Vec::new() }
    }

    /// `DATA/SD_CHR.OBJ`: the scene and staff scripts.
    pub fn set_sd_chr(&mut self, data: &[u8]) -> Result<(), JsError> {
        self.sd = Some(SdBank::parse(data).map_err(crate::js_err)?);
        Ok(())
    }

    /// Advance one frame (the ending takes no input).
    pub fn step(&mut self) {
        self.e.step();
        self.sounds.clear();
        let Some(bank) = &self.sd else { return };
        if self.e.kill {
            for o in &mut self.stage.objects[..self.e.count] {
                o.layer = -1;
            }
        }
        if let Some(id) = self.e.start {
            self.stage.start(bank, id);
        }
        if self.e.draw && self.stage.active {
            self.sounds = self.stage.step(bank);
        }
    }

    pub fn sounds(&self) -> Vec<u8> {
        self.sounds.clone()
    }

    /// True once the ending is over (mode 0: the title).
    #[wasm_bindgen(getter)]
    pub fn done(&self) -> bool {
        self.e.done
    }

    /// An XA request this frame (`func_80040A7C`: 9 the scene, 8 the staff roll), else −1.
    #[wasm_bindgen(getter)]
    pub fn xa(&self) -> i32 {
        self.e.xa.map_or(-1, i32::from)
    }

    /// `D_80100410`: 1 the scene, 2 its fade, 6 the staff roll (0, 3–5 load, nothing drawn).
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> u32 {
        u32::from(self.e.state)
    }

    /// The staff page on screen (1–10), 0 before the staff roll.
    #[wasm_bindgen(getter)]
    pub fn staff_page(&self) -> u32 {
        match self.e.art {
            Art::Staff if self.e.staff >= 0x155 => u32::from(self.e.staff - 0x154).min(10),
            _ => 0,
        }
    }
}
