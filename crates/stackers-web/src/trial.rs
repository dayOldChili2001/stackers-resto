//! Time Trial (mode 11) exposed to JavaScript: see `stackers_core::trial`. Drawn by
//! `Screen::trial`.

use crate::sprites::Sprites;
use stackers_core::trial::{Finish, Sub, TimeTrial, COURSES};
use stackers_core::Outcome;
use stackers_formats::presets::Presets;
use stackers_formats::sdchr::{self, SdBank, SdStage};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Trial {
    trial: TimeTrial,
    sd: Option<SdBank>,
    stage: SdStage,
    /// The mascot (actor 0, `func_80031910(0)`).
    mascot: SdStage,
    /// The `Mascot::starts` the actor was last started for.
    mascot_starts: u32,
    sounds: Vec<u8>,
    /// Script mirrors for player 2's field (cloned from the page's `Sprites` on first draw).
    p2_sprites: Option<Sprites>,
    last_sub: Sub,
}

impl Trial {
    pub(crate) fn trial(&self) -> &TimeTrial {
        &self.trial
    }

    pub(crate) fn bank(&self) -> Option<&SdBank> {
        self.sd.as_ref()
    }

    pub(crate) fn stage(&self) -> &SdStage {
        &self.stage
    }

    /// The mascot's objects (none on the course list or without `SD_CHR.OBJ`).
    pub(crate) fn mascot(&self) -> Option<&SdStage> {
        (self.sd.is_some() && self.trial.sub != Sub::Select && self.mascot.active).then_some(&self.mascot)
    }

    /// Player 2's sprite mirrors, made from `like` the first time.
    pub(crate) fn p2_sprites(&mut self, like: &Sprites) -> &mut Sprites {
        self.p2_sprites.get_or_insert_with(|| {
            let mut s = like.clone();
            s.reset();
            s
        })
    }
}

#[wasm_bindgen]
impl Trial {
    /// A Time Trial on `course` (0–4). `fil2_dat` is `DERODERO/DATA/FIL2_DAT.OBJ` (the course
    /// layouts); `best` holds the five best times (packed like the clock, 0 = none).
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32, fil2_dat: &[u8], course: u32, best: Vec<u32>) -> Result<Trial, JsError> {
        let layouts = Presets::parse(fil2_dat).and_then(|p| p.time_trial()).map_err(crate::js_err)?;
        let mut b = [0u32; COURSES];
        for (d, s) in b.iter_mut().zip(best) {
            *d = s;
        }
        Ok(Trial {
            trial: TimeTrial::new(seed, layouts, course as usize, b),
            sd: None,
            stage: SdStage::default(),
            mascot: SdStage::actor(),
            mascot_starts: 0,
            sounds: Vec::new(),
            p2_sprites: None,
            last_sub: Sub::Play,
        })
    }

    /// `DATA/SD_CHR.OBJ`: the result, countdown and course-list scripts.
    pub fn set_sd_chr(&mut self, data: &[u8]) -> Result<(), JsError> {
        self.sd = Some(SdBank::parse(data).map_err(crate::js_err)?);
        Ok(())
    }

    /// Which ports' connection state changed since the last step (the page compares the
    /// gamepads assigned to pads 1 and 2); while the clock runs, a change pauses.
    pub fn set_port_change(&mut self, p1: bool, p2: bool) {
        self.trial.port_change = [p1, p2];
    }

    /// The Option screen's button layouts (`D_80100374`) for pad 1 and pad 2, 0–2; call before
    /// [`Self::step`]. Only the play reads them (pause reads the raw pad).
    pub fn set_layouts(&mut self, p1: u8, p2: u8) {
        self.trial.versus.layouts = [p1.min(2), p2.min(2)];
    }

    /// Advance one frame with pad 1's buttons (`stackers_core::pad` bits).
    pub fn step(&mut self, buttons: u16) {
        self.trial.step(buttons);
        self.sounds.clone_from(&self.trial.sounds);
        let sub = self.trial.sub;
        if sub != self.last_sub {
            if sub == Sub::Play {
                // A new run: the course list's objects are gone, the fields start over.
                self.stage = SdStage::default();
                self.p2_sprites = None;
            }
            self.last_sub = sub;
        }
        let Some(bank) = &self.sd else { return };
        // func_800312AC: (re)start the actor when its mood changed or a course began, then
        // run it one frame (not while paused, not on the course list).
        let m = &self.trial.mascot;
        if sub != Sub::Select {
            if m.starts != self.mascot_starts {
                self.mascot_starts = m.starts;
                self.mascot.start_actor(bank, bank.actor_script(sdchr::TRIAL_ACTOR, m.mood));
            }
            if sub != Sub::Paused {
                for id in self.mascot.step(bank) {
                    if !self.sounds.contains(&id) {
                        self.sounds.push(id);
                    }
                }
            }
        }
        for &id in &self.trial.scripts {
            self.stage.start(bank, id);
        }
        if sub == Sub::Over {
            for &id in &self.trial.over.placed {
                self.stage.start_placed(bank, id);
            }
        }
        let run = match sub {
            Sub::Over => self.trial.over.active,
            Sub::Select => self.trial.select.run_sd,
            _ => false,
        };
        if run && self.stage.active {
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
        stackers_core::bgm::code(self.trial.music)
    }

    /// Sound-effect ids of the last step.
    pub fn sounds(&self) -> Vec<u8> {
        self.sounds.clone()
    }

    /// 0 still in the mode, 2 back to the title (Quit, or the countdown ran out).
    #[wasm_bindgen(getter)]
    pub fn outcome(&self) -> u32 {
        match self.trial.outcome {
            None => 0,
            Some(Outcome::Retry) => 1,
            Some(Outcome::Title) => 2,
        }
    }

    /// Best time per course (packed like [`Trial::clock`]; 0 = none). Updated on a record.
    pub fn best(&self) -> Vec<u32> {
        self.trial.best.to_vec()
    }

    /// The clock, one byte each (low to high): frames, seconds, minutes, hours.
    #[wasm_bindgen(getter)]
    pub fn clock(&self) -> u32 {
        self.trial.clock()
    }

    #[wasm_bindgen(getter)]
    pub fn course(&self) -> u32 {
        self.trial.course as u32
    }

    /// 0 running, 1 lost (topped out), 2 finished slower than the best, 3 new record.
    #[wasm_bindgen(getter)]
    pub fn finish(&self) -> u32 {
        match self.trial.finish {
            None => 0,
            Some(Finish::Lost) => 1,
            Some(Finish::Slower) => 2,
            Some(Finish::Record) => 3,
        }
    }

    /// `D_800C34AA`: 12 course list, 1 play, 3 pause, 4 end.
    #[wasm_bindgen(getter)]
    pub fn sub(&self) -> u32 {
        match self.trial.sub {
            Sub::Select => 12,
            Sub::Play => 1,
            Sub::Paused => 3,
            Sub::Over => 4,
        }
    }

    /// Settled blobs in player 2's field (the run ends at 72).
    #[wasm_bindgen(getter)]
    pub fn target(&self) -> u32 {
        u32::from(self.trial.versus.games[1].settled())
    }

    pub fn frame(&self) -> u32 {
        self.trial.frame as u32
    }

    /// Testing aid: fill player 2's play area (rows 6–17) with settled colour-6 blobs, so
    /// the next end check finishes the run.
    pub fn debug_fill_target(&mut self) {
        let f = &mut self.trial.versus.games[1].field;
        for c in 1..7 {
            for r in 6..18 {
                f.put(c, r, 6);
            }
        }
    }

    /// Testing aid: start player 1's all-clear sprite (mode 11's entry).
    #[doc(hidden)]
    pub fn debug_all_clear(&mut self) {
        self.trial.versus.games[0].all_clear = Some(stackers_core::AllClear { entry: 1, steps: 0 });
    }

    /// Player 1's pair: `[active, x, y, pivot, pieces so far]`.
    pub fn piece(&self) -> Vec<i32> {
        let g = &self.trial.versus.games[0];
        vec![i32::from(g.piece_active()), g.player.x, g.player.y, i32::from(g.player.pivot), g.stats.pieces as i32]
    }
}
