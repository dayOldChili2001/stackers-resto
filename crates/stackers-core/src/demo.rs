//! The attract demo (mode 2, `func_80018F24`): what the title falls into after 40 idle seconds.
//!
//! Two CPU players (`func_80034DFC`, the vs CPU AI with records `D_80055970` / `D_80055994`)
//! play a set on level row 3 with the two-player strip, no HUD, no mascot, no "Ready Go" and no
//! music. Every 300 frames of play with the clock running, play stops for an explanation panel
//! (`DEMO/SETUMEI0–3.TIM`, SD scripts 0x47–0x4D, draw mode 4: semi-transparent). After the last
//! panel, or as soon as Start is held on either pad, the game goes back to mode 0 (the title).
//!
//! Sub-states (`D_800C34AA`, `jtbl_800103C0`): 0 init, 1 play, 2 set result, 3 panel, 4 match
//! end (`func_8002D114` phase 6: 300 frames of burst, then sub-state 0 again, panels from the
//! first). The match is first to `D_80103692 / 2 + 1` sets; `D_80103692` is 0 at boot (and
//! only the Arcade set-match menu changes it), so the port plays one set and never reaches
//! sub-state 2.

use crate::cpu::{Cpu, Params};
use crate::game::pad;
use crate::roulette::{ColumnTally, Roulette};
use crate::versus::Versus;

/// Frames of play (`D_801003DC`, with the clock running) before the next panel.
pub const PANEL_AFTER: u32 = 0x12C;
/// How long a panel stays (`D_800C2F84`): 350 frames, 400 for the last.
pub const PANEL_FRAMES: u32 = 0x15E;
pub const LAST_PANEL_FRAMES: u32 = 0x190;
/// SD script of panel `n` (`D_800C3514 = 0x147 + D_80100360`).
pub const PANEL_SCRIPT: u8 = 0x47;
/// `D_80100360` runs 0, 1, 3, 4, 5, 6 (2 is skipped: SETUMEI1's top half is never shown);
/// reaching 7 ends the demo.
pub const PANELS_END: u8 = 7;
/// SE played as a panel opens (`func_80040740(0x12, 1)`).
pub const SE_PANEL: u8 = 0x12;
/// `D_800B1A28` set by `func_8002D0C4`: the match end's length.
pub const MATCH_END_FRAMES: u32 = 0x12C;
/// The CPU records: `D_8005588C[1][5]` for player 1, `[1][8]` for player 2.
pub const CPU_RECORDS: [(usize, usize); 2] = [(1, 5), (1, 8)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sub {
    /// Sub-state 0: the next frame sets up a fresh set (nothing moves, the AI doesn't run).
    Init,
    Play,
    Panel,
    MatchEnd,
}

#[derive(Clone, Debug)]
pub struct Demo {
    pub versus: Versus,
    pub sub: Sub,
    /// `D_80100360`: the next panel.
    pub panel: u8,
    /// `D_801003DC`.
    pub timer: u32,
    /// `D_800C2F84`: this panel's length.
    panel_frames: u32,
    /// `D_800C3530`: the match end's counter.
    end_timer: u32,
    pub winner: Option<usize>,
    pub roulette: [Roulette; 2],
    pub tallies: [[ColumnTally; 6]; 2],
    /// `D_8010ADA8[p]`: the AI's idle countdowns, carried into each restart.
    wiggle: [i16; 2],
    pub sounds: Vec<u8>,
    /// SD scripts requested this frame.
    pub scripts: Vec<u8>,
    /// Start was held or the panels ran out: back to the title (mode 0).
    pub done: bool,
    pub frame: u64,
}

impl Demo {
    /// Mode 2 from the title, after its sub-state 0 frame: `seed` is the BIOS `rand()` state.
    pub fn new(seed: u32) -> Demo {
        let mut d = Demo {
            versus: Versus::new(seed, [crate::tables::SOLO_ROW; 2]),
            sub: Sub::Play,
            panel: 0,
            // The title's state 5 left it at 4; sub-state 0 doesn't reset it.
            timer: 4,
            panel_frames: 0,
            end_timer: 0,
            winner: None,
            roulette: [Roulette::new(); 2],
            tallies: Default::default(),
            wiggle: [0; 2],
            sounds: Vec::new(),
            scripts: Vec::new(),
            done: false,
            frame: 0,
        };
        d.init(seed);
        d
    }

    /// Sub-state 0: level row 3 for both (`func_80039CEC`), fresh players (`func_8001FC64`), the
    /// strip (`func_8003B958`) and the AI records (`func_80035088`: mode 1, no buttons).
    fn init(&mut self, seed: u32) {
        for (w, c) in self.wiggle.iter_mut().zip(&self.versus.cpu) {
            if let Some(c) = c {
                *w = c.wiggle;
            }
        }
        self.versus = Versus::new(seed, [crate::tables::SOLO_ROW; 2]);
        for p in 0..2 {
            let (variant, stage) = CPU_RECORDS[p];
            self.versus.cpu[p] = Some(Cpu::new(Params::stage(variant, stage), self.wiggle[p]));
        }
        (self.roulette, self.tallies) = crate::roulette::new_strip(&mut self.versus);
        self.sub = Sub::Play;
        self.panel = 0;
        self.winner = None;
    }

    /// One 60 Hz frame with both pads' held buttons (only Start matters).
    pub fn step(&mut self, held: [u16; 2]) {
        self.sounds.clear();
        self.scripts.clear();
        self.frame += 1;
        if self.done {
            return;
        }
        match self.sub {
            Sub::Init => {
                let seed = self.versus.seed();
                self.init(seed);
            }
            Sub::Play => self.play(held),
            Sub::Panel => {
                // The pad read still runs the AI; nothing else moves.
                let h = self.versus.cpu_pads(held);
                self.versus.hold(h);
                self.timer += 1;
                self.step_cut_ins();
                if self.timer == self.panel_frames {
                    self.panel += 1;
                    if self.panel == 2 {
                        self.panel = 3;
                    }
                    self.timer = 0;
                    self.sub = Sub::Play;
                }
            }
            Sub::MatchEnd => {
                let h = self.versus.cpu_pads(held);
                self.versus.hold(h);
                self.end_timer += 1;
                if self.end_timer >= MATCH_END_FRAMES {
                    // Phase 6: wins and tallies cleared, sub-state 0 next frame.
                    self.end_timer = 0;
                    self.timer = 0;
                    self.sub = Sub::Init;
                }
                self.step_end_fields();
            }
        }
        self.exit_check(held);
    }

    /// The end of `func_80018F24`: the panels ran out or Start is held on either pad.
    fn exit_check(&mut self, held: [u16; 2]) {
        if self.panel == PANELS_END || held.iter().any(|&h| h & pad::START != 0) {
            self.done = true;
        }
    }

    /// Sub-state 1.
    fn play(&mut self, held: [u16; 2]) {
        self.timer += 1;
        // The AI holds the pads (Start and Select still come through); presses are computed
        // from the merged word, as the CPU's own.
        let before = self.versus.games.each_ref().map(|g| g.prev_held);
        self.versus.step(held);
        let after = self.versus.games.each_ref().map(|g| g.prev_held);
        let pressed = [after[0] & !before[0], after[1] & !before[1]];
        let mut sounds = Vec::new();
        for g in &self.versus.games {
            for &id in &g.sounds {
                if !sounds.contains(&id) {
                    sounds.push(id);
                }
            }
        }
        crate::roulette::step_strip(&mut self.roulette, &mut self.tallies, &mut self.versus, pressed, &mut sounds);
        self.sounds = sounds;
        let running = self.versus.games[0].clock_running();
        if (running && self.timer >= PANEL_AFTER) || self.panel == PANELS_END {
            self.sounds.push(SE_PANEL);
            self.timer = 0;
            self.scripts.push(PANEL_SCRIPT + self.panel);
            self.panel_frames = if self.panel == 6 { LAST_PANEL_FRAMES } else { PANEL_FRAMES };
            self.sub = Sub::Panel;
        }
        self.end_check();
    }

    /// `func_8003CBAC` → `func_8002DD28` (both out on one frame counts as player 1's win).
    fn end_check(&mut self) {
        let Some(winner) = self.versus.set_winner() else { return };
        self.winner = Some(winner);
        crate::roulette::stop_spins(&mut self.roulette);
        self.versus.finish(winner);
        self.sounds.push(crate::vs_cpu::se::ROUND_END);
        // func_8002D0C4: the match is won. `func_8002D91C` requests no emblem in mode 2 (and
        // clears the request, so a panel asked for this frame never opens).
        self.sub = Sub::MatchEnd;
        self.end_timer = 0;
        self.scripts.clear();
    }

    /// `func_8003B03C`: the loser's burst and the winner's pops.
    fn step_end_fields(&mut self) {
        let Some(w) = self.winner else { return };
        self.versus.games[1 - w].field.burst_step();
        let popped = self.versus.games[w].field.pop_special_step();
        // func_8003B644 always takes them off player 1's grey count.
        let f = &mut self.versus.games[1].field;
        f.grey = f.grey.wrapping_sub(popped);
    }

    /// `func_80038A08` runs in the panel sub-state too.
    fn step_cut_ins(&mut self) {
        for g in &mut self.versus.games {
            g.step_cut_in();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels_come_every_300_frames_of_play_then_the_title() {
        let mut d = Demo::new(0x1234);
        let mut shown = Vec::new();
        let mut play = 0;
        for _ in 0..20_000 {
            d.step([0, 0]);
            if d.sub == Sub::Play && d.versus.games[0].clock_running() {
                play += 1;
            }
            if let Some(&id) = d.scripts.first() {
                shown.push((id, play));
                play = 0;
            }
            if d.done {
                break;
            }
        }
        assert!(d.done);
        let ids: Vec<u8> = shown.iter().map(|s| s.0).collect();
        assert_eq!(ids, [0x47, 0x48, 0x4A, 0x4B, 0x4C, 0x4D]);
        // After the first, each panel follows 300 frames of play.
        assert!(shown[1..].iter().all(|s| s.1 == 300), "{shown:?}");
    }

    #[test]
    fn start_leaves_at_once() {
        let mut d = Demo::new(7);
        d.step([0, 0]);
        assert!(!d.done);
        d.step([0, pad::START]);
        assert!(d.done);
    }

    /// `FRAME_DUMP=/tmp/d.txt cargo test -p stackers-core demo::tests::frame_dump -- --ignored`
    /// then `python3 tools/framediff.py demo /tmp/d.txt`: whole demos (both sides the AI, no
    /// pads) from random seeds, until the panels run out.
    #[test]
    #[ignore]
    fn frame_dump() {
        use crate::rng::BiosRand;
        use crate::set_match::tests::versus_line;
        use std::fmt::Write as _;
        let path = std::env::var("FRAME_DUMP").expect("FRAME_DUMP");
        let games: u32 = std::env::var("FRAME_GAMES").map_or(4, |v| v.parse().unwrap());
        let mut rng = BiosRand::new(std::env::var("FRAME_SEED").map_or(333, |v| v.parse().unwrap()));
        let mut out = String::new();
        for _ in 0..games {
            let seed = rng.next() * 7919 + rng.next();
            let mut d = Demo::new(seed);
            let mut lines = vec![versus_line(&d.versus)];
            while !d.done && lines.len() < 30_000 {
                d.step([0, 0]);
                lines.push(versus_line(&d.versus));
            }
            writeln!(out, "D {seed} {}", lines.len() - 1).unwrap();
            for l in lines {
                writeln!(out, "F {l}").unwrap();
            }
        }
        std::fs::write(path, out).unwrap();
    }
}
