//! Arcade's vs CPU (mode 13) exposed to JavaScript: see `stackers_core::vs_cpu`. Drawn by
//! `Screen::vs_cpu`.

use crate::sprites::Sprites;
use stackers_core::set_match::SetMatch;
use stackers_core::vs_cpu::{Exit, Sub, VsCpu};
use stackers_formats::sdchr::{self, SdBank, SdStage};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct CpuMatch {
    m: VsCpu,
    sd: Option<SdBank>,
    stage: SdStage,
    sounds: Vec<u8>,
    p2_sprites: Option<Sprites>,
    /// A new round: the fields' script mirrors start over.
    fresh: bool,
    /// The CPU side's actor (`func_80031910`), and the core's restart count it last saw.
    mascot: SdStage,
    mascot_starts: u32,
    /// The stage whose art and actor table are loaded. The core bumps its stage on the winning
    /// frame (`D_800662F0`), but the PS1 keeps the beaten stage's background and sheets in VRAM
    /// (and its actor table, `func_800319CC`) until the next stage's init and load.
    art_stage: u8,
    /// This frame's effects from the actor script's sound op (`func_800328FC`).
    actor_sounds: Vec<u8>,
}

impl CpuMatch {
    pub(crate) fn from_core(m: VsCpu) -> CpuMatch {
        CpuMatch {
            sd: None,
            stage: SdStage::default(),
            sounds: Vec::new(),
            p2_sprites: None,
            fresh: true,
            mascot: SdStage::actor(),
            mascot_starts: 0,
            art_stage: m.stage,
            actor_sounds: Vec::new(),
            m,
        }
    }

    pub(crate) fn inner(&self) -> &VsCpu {
        &self.m
    }

    /// The core state, for tools that set up a scene (`examples/screen.rs`).
    #[doc(hidden)]
    pub fn core_mut(&mut self) -> &mut VsCpu {
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

    /// The mascot's objects (none while the stage loads or without `SD_CHR.OBJ`).
    pub(crate) fn mascot(&self) -> Option<&SdStage> {
        (self.sd.is_some() && self.m.sub != Sub::Load && self.mascot.active).then_some(&self.mascot)
    }

    /// The stage whose art is on screen (see `art_stage`), 0–13.
    pub(crate) fn art_stage(&self) -> usize {
        usize::from(self.art_stage.min(13))
    }

    pub(crate) fn take_fresh(&mut self) -> bool {
        std::mem::take(&mut self.fresh)
    }
}

#[wasm_bindgen]
impl CpuMatch {
    /// A vs CPU course: the human on pad `me` (0 or 1), course 0–2 from stage `stage` to
    /// `last_stage` (`FrontEnd::vs_course`).
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32, me: usize, course: u8, stage: u8, last_stage: u8) -> CpuMatch {
        CpuMatch::from_core(VsCpu::new(seed, me, course, stage, last_stage))
    }

    /// Once a challenger came in (`outcome` 4): mode 15's set match (call `set_sd_chr` on it).
    pub fn challenge(&self, seed: u32) -> Option<crate::Match> {
        match self.m.exit {
            Some(Exit::Challenger(c)) => Some(crate::Match::from_core(SetMatch::challenger(seed, c))),
            _ => None,
        }
    }

    /// Once the course is cleared (`outcome` 3): mode 16 (call `set_sd_chr` on it).
    pub fn ending(&self) -> Option<crate::Ending> {
        (self.m.exit == Some(Exit::Ending)).then(|| crate::Ending::new(self.m.course, self.m.last_stage))
    }

    /// `D_800C2FB8`: the human's pad.
    #[wasm_bindgen(getter)]
    pub fn menu_pad(&self) -> u32 {
        self.m.me as u32
    }

    /// `DATA/SD_CHR.OBJ`: "Ready Go" and the result scripts.
    pub fn set_sd_chr(&mut self, data: &[u8]) -> Result<(), JsError> {
        let bank = SdBank::parse(data).map_err(crate::js_err)?;
        self.stage.start(&bank, stackers_core::vs_cpu::script::READY);
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

    /// Advance one frame with each pad's buttons (the CPU's pad only counts for Start).
    pub fn step(&mut self, p1: u16, p2: u16) {
        let before = self.m.sub;
        // The mood reads the done flag the actor's last step left (func_80031910 runs after it).
        self.m.mascot_done = self.mascot.done;
        self.m.step([p1, p2]);
        self.sounds.clone_from(&self.m.sounds);
        self.actor_sounds.clear();
        if matches!(self.m.sub, Sub::Init { .. } | Sub::Load) {
            self.art_stage = self.m.stage;
        }
        if matches!(self.m.sub, Sub::Load | Sub::Play) && matches!(before, Sub::Init { .. }) {
            self.stage = SdStage::default();
            self.p2_sprites = None;
            self.fresh = true;
        }
        let Some(bank) = &self.sd else { return };
        // func_8003121C: (re)start the actor when its mood changed or a stage began, then step
        // it (not while paused or loading).
        if !matches!(self.m.sub, Sub::Load | Sub::Init { .. }) {
            let m = &self.m.mascot;
            if m.starts != self.mascot_starts {
                self.mascot_starts = m.starts;
                let table = sdchr::VS_ACTORS + 4 * self.art_stage() as u32;
                self.mascot.start_actor(bank, bank.actor_script(table, m.mood));
            }
            if self.m.sub != Sub::Paused {
                for id in self.mascot.step(bank) {
                    self.actor_sounds.push(id);
                    if !self.sounds.contains(&id) {
                        self.sounds.push(id);
                    }
                }
            }
        }
        for &id in &self.m.scripts {
            self.stage.start(bank, id);
        }
        for &id in &self.m.placed {
            self.stage.start_placed(bank, id);
        }
        if self.stage.active && !matches!(self.m.sub, Sub::Paused | Sub::Load | Sub::Init { .. }) {
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

    /// 0 still in the mode, 2 back to the title, 3 the ending (course cleared), 4 a challenger
    /// (mode 15: [`CpuMatch::challenge`]).
    #[wasm_bindgen(getter)]
    pub fn outcome(&self) -> u32 {
        match self.m.exit {
            None => 0,
            Some(Exit::Title) => 2,
            Some(Exit::Ending) => 3,
            Some(Exit::Challenger { .. }) => 4,
        }
    }

    /// `D_800C34AA`: 0 init, 1 play, 2 stage won, 3 pause, 4 lost (continue), 10 challenger,
    /// 11 loading.
    #[wasm_bindgen(getter)]
    pub fn sub(&self) -> u32 {
        match self.m.sub {
            Sub::Init { .. } => 0,
            Sub::Play => 1,
            Sub::Won => 2,
            Sub::Paused => 3,
            Sub::Lost => 4,
            Sub::Challenger => 10,
            Sub::Load => 11,
        }
    }

    /// `D_800662F0`.
    #[wasm_bindgen(getter)]
    pub fn stage_index(&self) -> u32 {
        u32::from(self.m.stage)
    }

    /// This frame's music request (`stackers_core::bgm::code`): −1 none, −2 stop, 0–99 a
    /// CD-DA table index, 100 + n XA entry n.
    #[wasm_bindgen(getter)]
    pub fn music(&self) -> i32 {
        stackers_core::bgm::code(self.m.music)
    }

    /// `[stage, last stage, score p1, score p2, bonus, rank (0 SPECIAL A … 5 LUCKY), clear frames]`.
    pub fn hud(&self) -> Vec<u32> {
        let m = &self.m;
        vec![
            u32::from(m.stage),
            u32::from(m.last_stage),
            m.versus.games[0].hud.score,
            m.versus.games[1].hud.score,
            m.bonus.points,
            m.bonus.rank() as u32,
            m.bonus.frames,
        ]
    }

    pub fn frame(&self) -> u32 {
        self.m.frame as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stackers_core::game::ps;

    /// The win bumps the core's stage at once, but the beaten stage's background and sheets
    /// stay up through the results until the next stage's init.
    #[test]
    fn art_stays_on_the_beaten_stage_until_the_next_init() {
        let mut c = CpuMatch::new(9, 1, 0, 2, 5);
        for _ in 0..3 {
            c.step(0, 0);
        }
        c.core_mut().versus.games[0].player.state = ps::GAME_OVER;
        c.step(0, 0);
        assert_eq!(c.inner().sub, Sub::Won);
        assert_eq!(c.inner().stage, 3);
        let mut won_frames = 0;
        while c.inner().sub == Sub::Won {
            assert_eq!(c.art_stage(), 2, "results drawn over the stage just beaten");
            c.step(0, 0);
            won_frames += 1;
            assert!(won_frames < 10_000, "never left the results");
        }
        assert!(matches!(c.inner().sub, Sub::Init { .. } | Sub::Load), "{:?}", c.inner().sub);
        assert_eq!(c.art_stage(), 3);
    }
}

#[cfg(test)]
mod actor_dump {
    use super::*;
    use stackers_core::rng::BiosRand;
    use stackers_core::game::pad;
    use stackers_core::vs_cpu::Course;
    use std::fmt::Write as _;

    /// Frames at which the CPU's pending garbage is topped up (6 per column; see
    /// `tools/framediff.py actor`).
    pub const GARBAGE_AT: [usize; 3] = [60, 700, 1400];

    /// The CPU actor's mood, done flag, last drop and sound-op effects per frame, for
    /// `python3 tools/framediff.py actor <dump>` against the original's `func_8003121C`.
    #[test]
    #[ignore]
    fn frame_dump() {
        let path = std::env::var("FRAME_DUMP").expect("FRAME_DUMP");
        let sd = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../extracted/disc/DERODERO/DATA/SD_CHR.OBJ")).unwrap();
        let games: u32 = std::env::var("FRAME_GAMES").map_or(4, |v| v.parse().unwrap());
        let mut rng = BiosRand::new(std::env::var("FRAME_SEED").map_or(4242, |v| v.parse().unwrap()));
        let mut out = String::new();
        let buttons = [pad::LEFT, pad::RIGHT, pad::DOWN, pad::CROSS, pad::CIRCLE];
        for _ in 0..games {
            let seed = rng.next() * 7919 + rng.next();
            let course = (rng.next() % 3) as u8;
            let (_, first, last) = stackers_core::front::arcade::VS_COURSES[course as usize];
            let stage = first + (rng.next() % u32::from(last - first)) as u8;
            let c = Course { menu_pad: 0, course, stage, last_stage: last, scores: [0; 2], all_clears: [0; 2], wiggle: 0, carry: Default::default() };
            let mut core = VsCpu::resume(seed, c);
            core.sub = Sub::Play;
            let mut m = CpuMatch::from_core(core);
            m.set_sd_chr(&sd).unwrap();
            let mut pads = Vec::new();
            let mut lines = String::new();
            let mut held = 0u16;
            while m.m.sub == Sub::Play && pads.len() < 2000 {
                if GARBAGE_AT.contains(&pads.len()) {
                    m.m.versus.games[1].garbage.pending = [0, 6, 6, 6, 6, 6, 6];
                }
                if rng.next() % 6 == 0 {
                    held = 0;
                    for &b in &buttons {
                        if rng.next() % 4 == 0 {
                            held |= b;
                        }
                    }
                }
                pads.push(held);
                m.step(held, 0);
                let mut ses = m.actor_sounds.clone();
                ses.sort_unstable();
                ses.dedup();
                writeln!(lines, "a {} {} {} {:?}", m.m.mascot.mood, u8::from(m.mascot.done), m.m.versus.games[1].garbage.dropped, ses).unwrap();
            }
            let pads: Vec<String> = pads.iter().map(u16::to_string).collect();
            writeln!(out, "A {seed} {course} {stage} {last} {}", pads.join(",")).unwrap();
            out += &lines;
        }
        std::fs::write(path, out).unwrap();
    }
}
