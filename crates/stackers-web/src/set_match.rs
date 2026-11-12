//! Arcade's two-player set match (mode 14, and mode 15 when a challenger interrupts vs CPU)
//! exposed to JavaScript: see `stackers_core::set_match`. Drawn by `Screen::set_match`.

use crate::sprites::Sprites;
use stackers_core::set_match::{SetMatch, Sub};
use stackers_core::Outcome;
use stackers_formats::sdchr::{SdBank, SdStage};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Match {
    m: SetMatch,
    sd: Option<SdBank>,
    stage: SdStage,
    sounds: Vec<u8>,
    /// Script mirrors for player 2's field (cloned from the page's `Sprites` on first draw).
    p2_sprites: Option<Sprites>,
    /// A new set: the fields' script mirrors start over.
    fresh: bool,
}

impl Match {
    pub(crate) fn from_core(m: SetMatch) -> Match {
        let mut s = Match { m, sd: None, stage: SdStage::default(), sounds: Vec::new(), p2_sprites: None, fresh: true };
        s.sounds.clone_from(&s.m.sounds);
        s
    }

    pub(crate) fn inner(&self) -> &SetMatch {
        &self.m
    }

    /// The core state, for tools that set up a scene (`examples/screen.rs`).
    #[doc(hidden)]
    pub fn core_mut(&mut self) -> &mut SetMatch {
        &mut self.m
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
impl Match {
    /// A set match: difficulty 1–5 and bombs 0–3 per player, `sets` 1, 3 or 5.
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32, d1: u8, d2: u8, b1: u8, b2: u8, sets: u8) -> Match {
        Match::from_core(SetMatch::new(seed, [d1, d2], [b1, b2], sets))
    }

    /// Mode 15 only: once the countdown ran out (`outcome` 3), the vs CPU course to go back to,
    /// with the match winner on the menu pad (call `set_sd_chr` on it).
    pub fn course(&self, seed: u32) -> Option<crate::CpuMatch> {
        self.m.back.map(|c| crate::CpuMatch::from_core(stackers_core::vs_cpu::VsCpu::resume(seed, c)))
    }

    /// Mode 15 (a challenger's match).
    #[wasm_bindgen(getter)]
    pub fn challenge(&self) -> bool {
        self.m.course.is_some()
    }

    /// `D_800C2FB8`.
    #[wasm_bindgen(getter)]
    pub fn menu_pad(&self) -> u32 {
        self.m.menu_pad as u32
    }

    /// `DATA/SD_CHR.OBJ`: "Ready Go" and the result scripts.
    pub fn set_sd_chr(&mut self, data: &[u8]) -> Result<(), JsError> {
        let bank = SdBank::parse(data).map_err(crate::js_err)?;
        // The first set's "Ready Go" was requested before the bank arrived.
        self.stage.start(&bank, stackers_core::set_match::script::READY);
        self.sd = Some(bank);
        Ok(())
    }

    /// Which ports' connection state changed since the last step (the page compares the
    /// gamepads assigned to pads 1 and 2); while the clock runs, a change pauses.
    pub fn set_port_change(&mut self, p1: bool, p2: bool) {
        self.m.port_change = [p1, p2];
    }

    /// The Option screen's button layouts (`D_80100374`) for pad 1 and pad 2, 0–2; call before
    /// [`Self::step`]. Only the play reads them (pause reads the raw pad).
    pub fn set_layouts(&mut self, p1: u8, p2: u8) {
        self.m.versus.layouts = [p1.min(2), p2.min(2)];
    }

    /// Advance one frame with each pad's buttons (`stackers_core::pad` bits).
    pub fn step(&mut self, p1: u16, p2: u16) {
        let before = (self.m.sub, self.m.wins, self.m.versus.frame);
        self.m.step([p1, p2]);
        self.sounds.clone_from(&self.m.sounds);
        if self.m.sub == Sub::Play && (before.0 != Sub::Play && before.0 != Sub::Paused) {
            self.stage = SdStage::default();
            self.p2_sprites = None;
            self.fresh = true;
        }
        let Some(bank) = &self.sd else { return };
        for &id in &self.m.scripts {
            self.stage.start(bank, id);
        }
        let run = match self.m.sub {
            Sub::Paused => false,
            Sub::Repick => self.m.run_sd,
            _ => true,
        };
        if self.stage.active && run {
            for id in self.stage.step(bank) {
                if !self.sounds.contains(&id) {
                    self.sounds.push(id);
                }
            }
        }
    }

    /// This frame's music request (`stackers_core::bgm::code`): −1 none, −2 stop, 0–99 a
    /// CD-DA table index, 100 + n XA entry n.
    #[wasm_bindgen(getter)]
    pub fn music(&self) -> i32 {
        stackers_core::bgm::code(self.m.music)
    }

    pub fn sounds(&self) -> Vec<u8> {
        self.sounds.clone()
    }

    /// 0 still in the mode, 2 back to the title (Quit, or the countdown ran out), 3 back to
    /// the vs CPU course (mode 15: [`Match::course`]).
    #[wasm_bindgen(getter)]
    pub fn outcome(&self) -> u32 {
        match self.m.outcome {
            None => 0,
            _ if self.m.back.is_some() => 3,
            Some(Outcome::Retry) => 1,
            Some(Outcome::Title) => 2,
        }
    }

    /// `D_800C34AA`: 1 play, 2 set result, 3 pause, 4 match end, 12 re-pick.
    #[wasm_bindgen(getter)]
    pub fn sub(&self) -> u32 {
        match self.m.sub {
            Sub::Play => 1,
            Sub::SetResult => 2,
            Sub::Paused => 3,
            Sub::MatchEnd => 4,
            Sub::Repick => 12,
        }
    }

    /// `[wins p1, wins p2, tally p1, tally p2, bombs p1, bombs p2, column p1, column p2]`
    /// (columns 1–6).
    pub fn hud(&self) -> Vec<u32> {
        let [a, b] = &self.m.versus.games;
        vec![
            u32::from(self.m.wins[0]),
            u32::from(self.m.wins[1]),
            u32::from(self.m.tally[0]),
            u32::from(self.m.tally[1]),
            u32::from(a.bombs),
            u32::from(b.bombs),
            u32::from(self.m.roulette[0].column) + 1,
            u32::from(self.m.roulette[1].column) + 1,
        ]
    }

    /// Re-pick screen: `[cursor p1, state p1, cursor p2, state p2, bombs p1, bombs p2, phase]`
    /// (phase: 0/1 the lists come in, 2 picking, 3 leaving).
    pub fn repick(&self) -> Vec<u32> {
        let r = &self.m.repick;
        vec![
            u32::from(r[0].cursor),
            u32::from(r[0].state),
            u32::from(r[1].cursor),
            u32::from(r[1].state),
            u32::from(self.m.bombs[0]),
            u32::from(self.m.bombs[1]),
            u32::from(self.m.phase),
        ]
    }

    pub fn frame(&self) -> u32 {
        self.m.frame as u32
    }
}
