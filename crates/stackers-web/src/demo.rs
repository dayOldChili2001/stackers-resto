//! The attract demo (mode 2) exposed to JavaScript: see `stackers_core::demo`. Drawn by
//! `Screen::demo` over vs CPU stage 6's background (`Screen::demo_background`).

use crate::sprites::Sprites;
use stackers_core::demo::{Demo as Core, Sub};
use stackers_formats::sdchr::{SdBank, SdStage};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Demo {
    d: Core,
    sd: Option<SdBank>,
    stage: SdStage,
    sounds: Vec<u8>,
    /// Script mirrors for player 2's field (cloned from the page's `Sprites` on first draw).
    p2_sprites: Option<Sprites>,
    /// A new set: the fields' script mirrors start over.
    fresh: bool,
}

impl Demo {
    pub(crate) fn inner(&self) -> &Core {
        &self.d
    }

    pub(crate) fn bank(&self) -> Option<&SdBank> {
        self.sd.as_ref()
    }

    pub(crate) fn stage(&self) -> &SdStage {
        &self.stage
    }

    pub(crate) fn p2_sprites(&mut self, like: &Sprites) -> &mut Sprites {
        self.p2_sprites.get_or_insert_with(|| {
            let mut s = like.clone();
            s.reset();
            s
        })
    }

    /// True once after a set starts (the page's player-1 `Sprites` should reset too).
    pub(crate) fn take_fresh(&mut self) -> bool {
        std::mem::take(&mut self.fresh)
    }
}

#[wasm_bindgen]
impl Demo {
    /// Mode 2, as the title hands over to it; `seed` is the BIOS `rand()` state.
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32) -> Demo {
        Demo { d: Core::new(seed), sd: None, stage: SdStage::default(), sounds: Vec::new(), p2_sprites: None, fresh: true }
    }

    /// `DATA/SD_CHR.OBJ`: the explanation panels' scripts.
    pub fn set_sd_chr(&mut self, data: &[u8]) -> Result<(), JsError> {
        self.sd = Some(SdBank::parse(data).map_err(crate::js_err)?);
        Ok(())
    }

    /// Advance one frame with each pad's buttons (only Start is read: it ends the demo).
    pub fn step(&mut self, p1: u16, p2: u16) {
        let before = self.d.sub;
        self.d.step([p1, p2]);
        self.sounds.clone_from(&self.d.sounds);
        if self.d.sub == Sub::Init || (before == Sub::Init && self.d.sub == Sub::Play) {
            self.stage = SdStage::default();
            self.p2_sprites = None;
            self.fresh = true;
        }
        let Some(bank) = &self.sd else { return };
        for &id in &self.d.scripts {
            self.stage.start(bank, id);
        }
        // Only the panel sub-state runs the VM (func_8002EBA0, draw mode 4); the request made
        // in the play sub-state starts there on the next frame.
        if before == Sub::Panel && self.d.sub == Sub::Panel && self.stage.active {
            for id in self.stage.step(bank) {
                if !self.sounds.contains(&id) {
                    self.sounds.push(id);
                }
            }
        }
    }

    pub fn sounds(&self) -> Vec<u8> {
        self.sounds.clone()
    }

    /// Start was held, or the last panel closed: back to the title (mode 0).
    #[wasm_bindgen(getter)]
    pub fn done(&self) -> bool {
        self.d.done
    }

    /// `D_800C34AA`: 0 init, 1 play, 3 panel, 4 match end.
    #[wasm_bindgen(getter)]
    pub fn sub(&self) -> u32 {
        match self.d.sub {
            Sub::Init => 0,
            Sub::Play => 1,
            Sub::Panel => 3,
            Sub::MatchEnd => 4,
        }
    }

    /// `D_80100360`: the next panel (0, 1, 3, 4, 5, 6).
    #[wasm_bindgen(getter)]
    pub fn panel(&self) -> u32 {
        u32::from(self.d.panel)
    }

    pub fn frame(&self) -> u32 {
        self.d.frame as u32
    }
}
