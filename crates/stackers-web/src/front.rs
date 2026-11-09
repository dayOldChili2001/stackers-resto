//! The title and mode menu exposed to JavaScript (see `stackers_core::front`).

use stackers_core::front::{self, Music};
use stackers_formats::sdchr::{SdBank, SdStage};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct FrontEnd {
    front: front::Front,
    sd: Option<SdBank>,
    stage: SdStage,
    sounds: Vec<u8>,
}

impl FrontEnd {
    pub(crate) fn front(&self) -> &front::Front {
        &self.front
    }

    pub(crate) fn stage(&self) -> &SdStage {
        &self.stage
    }
}

impl Default for FrontEnd {
    fn default() -> Self {
        FrontEnd::new()
    }
}

#[wasm_bindgen]
impl FrontEnd {
    /// Power-on: the blank frames, the TECMO logo, then the title.
    #[wasm_bindgen(constructor)]
    pub fn new() -> FrontEnd {
        FrontEnd { front: front::Front::new(), sd: None, stage: SdStage::default(), sounds: Vec::new() }
    }

    /// `DATA/SD_CHR.OBJ`: the title and menu are SD_CHR scripts.
    pub fn set_sd_chr(&mut self, data: &[u8]) -> Result<(), JsError> {
        self.sd = Some(SdBank::parse(data).map_err(crate::js_err)?);
        Ok(())
    }

    /// Advance one frame with pad 1. `buttons` uses the PS1 pad bits from `stackers_core::pad`.
    pub fn step(&mut self, buttons: u16) {
        self.step2(buttons, 0);
    }

    /// Advance one frame with both pads (the Arcade difficulty list reads pad 2).
    pub fn step2(&mut self, pad1: u16, pad2: u16) {
        self.front.step2([pad1, pad2]);
        self.sounds.clone_from(&self.front.sounds);
        // func_8002EC34 + func_8002EC00: start the pending request, then run every object.
        if let (Some(run), Some(bank)) = (self.front.sd, &self.sd) {
            if let Some(id) = run.start {
                self.stage.start(bank, id);
            }
            if self.stage.active {
                for id in self.stage.step(bank) {
                    if !self.sounds.contains(&id) {
                        self.sounds.push(id);
                    }
                }
            }
        }
    }

    /// Sound-effect ids triggered by the last step.
    pub fn sounds(&self) -> Vec<u8> {
        self.sounds.clone()
    }

    /// Music request from the last step: −1 none, −2 stop, else the CD-DA table index to play.
    #[wasm_bindgen(getter)]
    pub fn music(&self) -> i32 {
        match self.front.music {
            None => -1,
            Some(Music::Stop) => -2,
            Some(Music::Play(i)) => i32::from(i),
        }
    }

    /// The menu picked Insane Mode and the hand-off has finished: start a solo game.
    #[wasm_bindgen(getter)]
    pub fn launch_solo(&self) -> bool {
        self.front.launch_solo()
    }

    /// The menu picked Chain Reaction and the hand-off has finished: start a mode-7 game.
    #[wasm_bindgen(getter)]
    pub fn launch_chain(&self) -> bool {
        self.front.launch_chain()
    }

    /// The course list picked a Time Trial course and the hand-off has finished: start mode 11
    /// on [`FrontEnd::course`].
    #[wasm_bindgen(getter)]
    pub fn launch_trial(&self) -> bool {
        self.front.launch_trial()
    }

    /// The Arcade's two-player branch has finished: start a mode-14 set match with
    /// [`FrontEnd::difficulty`], [`FrontEnd::stock`] and [`FrontEnd::sets`].
    #[wasm_bindgen(getter)]
    pub fn launch_versus(&self) -> bool {
        self.front.launch_versus()
    }

    /// `D_80102740[player]`: 1 Easy … 5 Insanity.
    pub fn difficulty(&self, player: usize) -> u8 {
        self.front.difficulty[player.min(1)]
    }

    /// `D_800FFCF8[player]`: □ uses per set (0–3).
    pub fn stock(&self, player: usize) -> u8 {
        self.front.stock[player.min(1)]
    }

    /// `D_80103692`: sets in the match (1, 3 or 5).
    #[wasm_bindgen(getter)]
    pub fn sets(&self) -> u8 {
        self.front.sets
    }

    /// `D_800699D4`: the Time Trial course picked in mode 12 (0–4).
    #[wasm_bindgen(getter)]
    pub fn course(&self) -> u32 {
        u32::from(self.front.course)
    }

    /// `D_80100684`: 0 blank, 1 title, 8 menu, 9 hand-off, 12 Time Trial course list.
    #[wasm_bindgen(getter)]
    pub fn mode(&self) -> u8 {
        self.front.mode
    }
}
