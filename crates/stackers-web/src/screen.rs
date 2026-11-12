//! The whole 320×240 solo screen as one draw list: field, queue window, HUD panels and frame.
//!
//! The background ([`Screen::background`]) is a full-screen image the page draws first; this
//! list goes on top, back to front. The original sorts by OT layer (the frame is on layer 1,
//! everything else on layer 2, nothing here overlaps within a layer except as listed).

use crate::front::FrontEnd;
use crate::session::Session;
use crate::sprites::Sprites;
use crate::set_match::Match;
use crate::vs_cpu::CpuMatch;
use crate::trial::Trial;
use crate::demo::Demo;
use crate::ending::Ending;
use stackers_core::trial::Sub;
use stackers_core::Game;
use stackers_core::front::arcade::{STOCK_X, STOCK_Y};
use stackers_core::front::{option, Arrow, OptionDraw, Overlay, Splash, DESCRIPTION_AT, SLOTS};
use stackers_core::pause::{menu, BRIGHT_ON};
use crate::sprites::Part;
use stackers_formats::anm::{AnimBank, Frame, ScriptPlayer, HUD_DIGITS, HUD_MESSAGE, HUD_SPARKLE};
use stackers_core::roulette::{ColumnTally, Roulette};
use stackers_core::set_match::{SetMatch, Sub as MatchSub};
use stackers_formats::chain::{self, ChainScreen};
use stackers_formats::hud::{self, SoloScreen, Stage};
use stackers_formats::sdchr::{self, SdBank, SdStage};
use stackers_formats::title::{self, BgMap, TitleLayout};
use wasm_bindgen::prelude::*;

/// Ints per sprite in [`Screen::solo`]:
/// `[page, clut, u, v, w, h, x, y, draw_w, draw_h, flip, mode]`, `mode` = brightness (128 =
/// unchanged) | `semi << 8` (STP texels blend 50/50).
pub const SCREEN_STRIDE: usize = 12;

/// A sprite-pushing closure with the screen's page mapping (`push`'s arguments).
type Put<'a> = dyn Fn(&mut Vec<i32>, &Frame, i32, i32, i32, Option<i16>, i32) + 'a;

/// The two-player screens' page ids: SD images on 26/27, the set match's pages, else solo's.
fn vs_page(vnao: [u8; 2], p: i16) -> i32 {
    vs_page_on(None, vnao, p)
}

/// [`vs_page`], with vs CPU's stage: its own character sheets in the RAM slots.
fn vs_page_on(stage: Option<usize>, vnao: [u8; 2], p: i16) -> i32 {
    if let (Some(stage), 26 | 27) = (stage, p) {
        let n = vnao[usize::from(p == 27)];
        return hud::vs_cpu_sheet(stage, n).map_or(-1, |s| VS_PAGE_BASE + i32::from(s));
    }
    if let (26 | 27, n @ 1..=3) = (p, vnao[usize::from(p == 27)]) {
        VS_PAGE_BASE + i32::from(hud::vs_sd_page(n))
    } else if hud::VS_PAGES.contains(&p)
        || (44..=46).contains(&p)
        || (hud::DEMO_PANEL_SLOT..hud::DEMO_PANEL_SLOT + 4).contains(&p)
        || p >= hud::VS_CPU_FILE_SLOT
    {
        VS_PAGE_BASE + i32::from(p)
    } else {
        i32::from(p)
    }
}
const NORMAL: i32 = 128;
/// Page ids of the art Chain Reaction loads over solo slots ([`chain::PAGES`]) are this + slot.
pub const CHAIN_PAGE_BASE: i32 = 200;
/// Page ids of the art Time Trial loads over solo slots ([`hud::TRIAL_PAGES`]) are this + slot.
pub const TRIAL_PAGE_BASE: i32 = 300;
/// Page ids of the art the set match loads over solo slots ([`hud::VS_PAGES`]).
pub const VS_PAGE_BASE: i32 = 400;
/// Page ids of the ending's art are this + its file index ([`hud::ending_file_path`]).
pub const ENDING_PAGE_BASE: i32 = 600;
const SEMI: i32 = 1 << 8;
/// Page id of a flat-coloured rectangle (a GPU TILE): its clut field is the colour `0xRRGGBB`;
/// with [`SEMI`] in the mode it is blended half and half with what's under it.
pub const FILL_PAGE: i32 = -1;

#[wasm_bindgen]
pub struct Screen {
    layout: SoloScreen,
    chain: ChainScreen,
    title: TitleLayout,
    bg: Option<BgMap>,
    stage: Stage,
    record: u32,
}

/// One sprite the way `func_8001B148` places it: at `(x + dx·s, y + dy·s)`, sized
/// `(w + (s ≠ 128))·s`, `s` out of 128.
fn push(out: &mut Vec<i32>, f: &Frame, x: i32, y: i32, scale: i32, clut: Option<i16>, mode: i32) {
    if f.w <= 0 || f.h <= 0 {
        return;
    }
    let grow = i32::from(scale != 128);
    let sx = x + ((i32::from(f.dx) * scale) >> 7);
    let sy = y + ((i32::from(f.dy) * scale) >> 7);
    let dw = ((i32::from(f.w) + grow) * scale) >> 7;
    let dh = ((i32::from(f.h) + grow) * scale) >> 7;
    out.extend_from_slice(&[
        i32::from(f.page),
        i32::from(clut.unwrap_or(f.clut)),
        i32::from(f.u),
        i32::from(f.v),
        i32::from(f.w),
        i32::from(f.h),
        sx,
        sy,
        dw,
        dh,
        i32::from(f.flip != 0),
        mode,
    ]);
}

#[wasm_bindgen]
impl Screen {
    /// `exe` is `SLUS_003.15` from the user's disc (the layout records live in it).
    #[wasm_bindgen(constructor)]
    pub fn new(exe: &[u8]) -> Result<Screen, JsError> {
        Ok(Screen {
            layout: SoloScreen::from_exe(exe).map_err(crate::js_err)?,
            chain: ChainScreen::from_exe(exe).map_err(crate::js_err)?,
            title: TitleLayout::from_exe(exe).map_err(crate::js_err)?,
            bg: None,
            stage: Stage::St22,
            record: 0,
        })
    }

    /// The "Record" panel value (`D_800B0F3C`: the best erase number, kept by the page).
    #[wasm_bindgen(setter)]
    pub fn set_record(&mut self, record: u32) {
        self.record = record;
    }

    /// Disc path of the full-screen background drawn under everything.
    pub fn background(&self) -> String {
        hud::background(self.stage)
    }

    /// Disc path of the TIM behind page id `page` (solo pages are texture-page slots; title
    /// pages are `stackers_formats::title::PAGE_BASE` and up).
    pub fn page_file(&self, page: i32) -> Option<String> {
        if page >= ENDING_PAGE_BASE {
            return hud::ending_file_path(u8::try_from(page - ENDING_PAGE_BASE).ok()?).map(String::from);
        }
        if page >= VS_PAGE_BASE {
            return hud::vs_page_file(i16::try_from(page - VS_PAGE_BASE).ok()?).map(String::from);
        }
        if page >= TRIAL_PAGE_BASE {
            return hud::trial_page_file(i16::try_from(page - TRIAL_PAGE_BASE).ok()?).map(String::from);
        }
        if page >= CHAIN_PAGE_BASE {
            return chain::page_file(i16::try_from(page - CHAIN_PAGE_BASE).ok()?).map(String::from);
        }
        if page >= title::PAGE_BASE {
            return title::page_file(page).map(String::from);
        }
        hud::page_file(i16::try_from(page).ok()?, self.stage)
    }

    /// Every page id [`Screen::solo`] and [`Screen::title`] can reference.
    pub fn pages(&self) -> Vec<i32> {
        let mut pages: Vec<i32> = (0..32).filter(|&p| self.page_file(p).is_some()).collect();
        pages.extend(title::pages());
        pages.extend(chain::PAGES.iter().map(|&p| CHAIN_PAGE_BASE + i32::from(p)));
        pages.extend(hud::TRIAL_PAGES.iter().map(|&p| TRIAL_PAGE_BASE + i32::from(p)));
        pages.extend(hud::VS_FILES.iter().map(|&p| VS_PAGE_BASE + i32::from(p)));
        pages.extend([44, 45, 46].map(|p| VS_PAGE_BASE + p));
        // The attract demo's panels (pages 14–17).
        pages.extend((0..4).map(|i| VS_PAGE_BASE + i32::from(hud::DEMO_PANEL_SLOT) + i));
        // vs CPU's page-18 art (the stages' B files) and opponents; the A files are backgrounds.
        pages.extend(
            (0x5B..=0x7Cu8)
                .filter(|&id| id >= 0x75 || (id - 0x5B) % 2 == 1)
                .map(|id| VS_PAGE_BASE + i32::from(hud::vs_cpu_slot(id))),
        );
        // vs CPU's character sheets (RAM slots 1–4 by stage).
        pages.extend((0..hud::VS_CPU_SHEET_FILES.len() as i32).map(|i| VS_PAGE_BASE + i32::from(hud::VS_CPU_SHEET_SLOT) + i));
        pages.extend(hud::ENDING_FILES.iter().map(|&id| ENDING_PAGE_BASE + i32::from(id)));
        pages
    }

    /// The ending (mode 16) for this frame: its SD objects in draw mode 3 (`func_80030070`:
    /// frame palettes, brightness `D_80103610[slot]`), slot 0 on top, on black. Empty on the
    /// frames the original draws nothing.
    pub fn ending(&self, e: &Ending) -> Vec<i32> {
        let mut out = Vec::new();
        let core = e.inner();
        if !core.draw {
            return out;
        }
        let (course, staff) = match core.art {
            stackers_core::ending::Art::Scene { course } => (course, false),
            stackers_core::ending::Art::Staff => (core.course, true),
            stackers_core::ending::Art::None => return out,
        };
        for (slot, o) in e.stage().objects.iter().enumerate().rev() {
            let Some(f) = o.frame.filter(|_| o.visible) else { continue };
            if f.w <= 0 || f.h <= 0 {
                continue;
            }
            let Some(id) = hud::ending_page(course, staff, f.page) else { continue };
            let (mut x, y, mut dw, dh, mut flip) = sdchr::placement(o, &f);
            if dw == 0 || dh <= 0 {
                continue;
            }
            // A negative scale draws the quad right to left: mirrored.
            if dw < 0 {
                x += dw;
                dw = -dw;
                flip = !flip;
            }
            out.extend_from_slice(&[
                ENDING_PAGE_BASE + i32::from(id),
                i32::from(f.clut),
                i32::from(f.u),
                i32::from(f.v),
                i32::from(f.w),
                i32::from(f.h),
                x,
                y,
                dw,
                dh,
                i32::from(flip),
                i32::from(core.bright[slot]),
            ]);
        }
        out
    }

    /// Disc path of Time Trial's full-screen background (ST53A), drawn under [`Screen::trial`].
    pub fn trial_background(&self) -> String {
        hud::TRIAL_BACKGROUND.into()
    }

    /// The Time Trial screen (mode 11) for this frame, back to front, over
    /// [`Screen::trial_background`]: both fields, the next-pair window, the garbage field's frame,
    /// the clock monitor, the W_ZERO frame, the clock, then the pause menu or the SD scripts.
    pub fn trial(&self, sprites: &mut Sprites, trial: &mut Trial) -> Vec<i32> {
        let page = |p: i16| -> i32 {
            if hud::TRIAL_PAGES.contains(&p) {
                TRIAL_PAGE_BASE + i32::from(p)
            } else if p == 28 || p == 29 {
                // RENSA1/2, as in the set match.
                VS_PAGE_BASE + i32::from(p)
            } else {
                i32::from(p)
            }
        };
        let put = |out: &mut Vec<i32>, f: &Frame, x: i32, y: i32, s: i32, clut: Option<i16>, mode: i32| {
            let at = out.len();
            push(out, f, x, y, s, clut, mode);
            if out.len() > at {
                out[at] = page(f.page);
            }
        };
        let mut out = Vec::with_capacity(220 * SCREEN_STRIDE);
        let (ox, oy) = hud::FIELD_ORIGIN;
        let sub = trial.trial().sub;
        let fields = matches!(sub, Sub::Play | Sub::Over);

        // OT layer 2, latest added first: the cut-ins, player 2's field (`func_800195FC(2)`),
        // player 1's field and pair (hidden while paused), the queue, then the garbage frame
        // and the monitor that `func_8001A70C` added first.
        if sub == Sub::Play {
            let games = &trial.trial().versus.games;
            for (f, x, y, s) in Self::cut_ins(trial.bank(), &[&games[0], &games[1]], false) {
                put(&mut out, &f, x, y, s, None, NORMAL);
            }
        }
        if sub != Sub::Select {
            let p2 = trial.trial().versus.games[1].clone();
            let p2_sprites = trial.p2_sprites(sprites);
            for (f, x, y, _) in p2_sprites.field_frames(&p2) {
                put(&mut out, &f, x + ox + hud::P2_FIELD_DX, y + oy, 128, None, NORMAL);
            }
        }
        let p1: &Game = &trial.trial().versus.games[0];
        if fields {
            for (f, x, y, _) in sprites.field_frames(p1) {
                put(&mut out, &f, x + ox, y + oy, 128, None, NORMAL);
            }
        }
        if sub != Sub::Select {
            let frames = sprites.queue_frames(p1);
            for (&y, halves) in p1.queue_y.iter().zip(frames) {
                let scale = hud::queue_scale(y);
                let cell = scale / 8;
                let x = hud::QUEUE_X + (16 - cell) / 2;
                for (f, dy) in halves.iter().zip([0, cell]) {
                    if let Some(f) = f {
                        put(&mut out, f, x, y + hud::QUEUE_TOP + dy, scale, None, NORMAL);
                    }
                }
            }
            let (px, py) = hud::QUEUE_PATCH_AT;
            put(&mut out, &self.layout.queue_patch, px, py, 128, None, hud::QUEUE_PATCH_BRIGHTNESS);
        }
        let (gx, gy) = hud::GARBAGE_FRAME_AT;
        put(&mut out, &self.layout.garbage_frame, gx, gy, 128, None, NORMAL);
        let (mx, my) = hud::TRIAL_MONITOR_AT;
        put(&mut out, &self.layout.panels[0], mx, my, 128, Some(hud::TRIAL_MONITOR_CLUT), NORMAL | SEMI);

        // Layer 1: player 1's arm spans (added after the frame, so under it), then W_ZERO.
        if fields {
            for (f, x, y, part) in sprites.span_frames(p1).iter().rev() {
                let Part::Span { clut } = *part else { continue };
                put(&mut out, f, x + ox, y + oy, 128, Some(clut), NORMAL);
            }
        }
        // The all-clear sprite: queued by the score step after W_ZERO (`func_8001A70C` runs
        // first) and before the spans, so between them.
        if sub == Sub::Play {
            let games = &trial.trial().versus.games;
            Self::all_clears(&mut out, &put, sprites, [&games[0], &games[1]]);
        }
        put(&mut out, &self.layout.window, 0, 0, 128, None, NORMAL);

        let t = trial.trial();
        if sub == Sub::Select {
            // Layer 0: the course list's arrow (`D_80056A88`, palette 3) over its scripts,
            // which draw with the list's palettes (`func_8002EAD4`, draw mode 1).
            // The arrow is added before the VM runs, so it is in front.
            let sel = &t.select;
            self.sd_objects(&mut out, trial.stage(), Some(&sel.palettes), &page);
            if sel.state >= 2 {
                if let Some(a) = self.layout.course_arrows.get(sel.arrow_frame) {
                    put(&mut out, a, sel.arrow.0, sel.arrow.1, 128, Some(3), NORMAL);
                }
            }
            return out;
        }

        // Layer 0: the clock and the best time (`func_8003D9B0`).
        let best = t.best[t.course];
        for (value, (x, y), clut) in [
            (t.clock(), hud::TRIAL_CLOCK_AT, hud::TRIAL_CLOCK_CLUT),
            (best, hud::TRIAL_BEST_AT, hud::TRIAL_BEST_CLUT),
        ] {
            let (m, s, f6, f2) = hud::trial_clock_digits(value);
            let x = self.number(&mut out, m, 2, x + self.layout.space, y, 1, clut, true);
            let x = self.glyph(&mut out, &self.layout.glyph_s, x, y, clut);
            let x = self.number(&mut out, s, 2, x, y, 1, clut, true);
            let x = self.glyph(&mut out, &self.layout.glyph_d, x, y, clut);
            let x = self.number(&mut out, f6, 1, x, y, 1, clut, true);
            self.number(&mut out, f2, 1, x, y, 1, clut, true);
        }
        // The mascot (`func_800312AC`, added before the clock, after the pause menu and the
        // result scripts).
        if let Some(stage) = trial.mascot() {
            Self::actor(&mut out, stage, &page, &title::vnao_page);
        }
        if sub == Sub::Paused {
            self.pause_menu(&mut out, &t.pause, &put);
        }
        if sub == Sub::Over {
            self.sd_objects(&mut out, trial.stage(), None, &page);
        }
        out
    }

    /// Disc path of the set match's full-screen background (ST18A), drawn under
    /// [`Screen::set_match`].
    pub fn vs_background(&self) -> String {
        hud::VS_BACKGROUND.into()
    }

    /// Mode 15's backgrounds (ST27A, ST28A).
    pub fn vs_challenge_backgrounds(&self) -> Vec<String> {
        hud::VS_CHALLENGE_BACKGROUNDS.iter().map(|&s| s.into()).collect()
    }

    /// Disc path of this match's background: ST18A, or in mode 15 the menu pad's.
    pub fn match_background(&self, m: &Match) -> String {
        let m = m.inner();
        match m.course {
            Some(_) => hud::VS_CHALLENGE_BACKGROUNDS[m.menu_pad & 1].into(),
            None => hud::VS_BACKGROUND.into(),
        }
    }

    /// The set-match screen (mode 14) for this frame, back to front, over
    /// [`Screen::vs_background`]: both fields and queues, the arm spans, the two-player frame,
    /// the bomb stocks and the "n SET MATCH" / wins / tallies HUD, then the pause menu and the SD
    /// scripts. `sprites` is player 1's (reset it when a set starts: see `fresh`).
    pub fn set_match(&self, sprites: &mut Sprites, m: &mut Match) -> Vec<i32> {
        let vnao = m.stage().vnao;
        let challenge = m.inner().course.map(|_| m.inner().menu_pad);
        let page = move |p: i16| vs_page(vnao, challenge.map_or(p, |pad| hud::vs_challenge_page(pad, p)));
        let put = |out: &mut Vec<i32>, f: &Frame, x: i32, y: i32, s: i32, clut: Option<i16>, mode: i32| {
            let at = out.len();
            push(out, f, x, y, s, clut, mode);
            if out.len() > at {
                out[at] = page(f.page);
            }
        };
        if m.take_fresh() {
            sprites.reset();
        }
        let mut out = Vec::with_capacity(300 * SCREEN_STRIDE);
        let sub = m.inner().sub;
        let repick = sub == MatchSub::Repick;
        let p1: Game = m.inner().versus.games[0].clone();
        let p2: Game = m.inner().versus.games[1].clone();
        // Only the play sub-state runs func_80038A08.
        let playing = sub == MatchSub::Play;
        let cut_ins = if playing { Self::cut_ins(m.bank(), &[&p1, &p2], false) } else { Vec::new() };
        if !repick {
            let p2s = m.p2_sprites(sprites);
            self.vs_fields(&mut out, &put, sprites, p2s, [&p1, &p2], cut_ins, playing);
        } else {
            self.vs_frame(&mut out, &put);
        }

        // Layer 0: the HUD (func_8001DB4C).
        let t = m.inner();
        let (sx, sy) = hud::VS_SETS_AT;
        self.number(&mut out, u32::from(t.sets), 1, sx, sy, 1, hud::VS_SETS_CLUT, false);
        self.text(&mut out, "SET", hud::VS_SET_LABEL_X, sy, hud::VS_SETS_CLUT);
        self.text(&mut out, "MATCH", hud::VS_MATCH_LABEL_X, sy, hud::VS_SETS_CLUT);
        for p in 0..2 {
            self.number(&mut out, u32::from(t.wins[p]), 1, hud::VS_WINS_X[p], hud::VS_WINS_Y, 1, hud::VS_WINS_CLUT, false);
            let (digits, x) = hud::vs_tally_at(p, u32::from(t.tally[p]));
            self.number(&mut out, u32::from(t.tally[p]), digits, x, hud::VS_TALLY_Y, 1, hud::VS_WINS_CLUT, true);
        }
        for (x, y) in hud::VS_TALLY_MARK_AT {
            put(&mut out, &hud::VS_TALLY_MARK, x, y, 128, Some(0), NORMAL);
        }
        self.glyph(&mut out, &self.layout.glyph_dash, hud::VS_DASH_X, hud::VS_WINS_Y, hud::VS_DASH_CLUT);
        // The bomb stocks (func_8001D778 in func_8001E020): label, digit on top.
        let stocks: [u32; 2] = if repick {
            [u32::from(t.bombs[0]), u32::from(t.bombs[1])]
        } else {
            [u32::from(p1.bombs), u32::from(p2.bombs)]
        };
        for p in 0..2 {
            let x = hud::VS_STOCK_X[p];
            let dx = if p == 0 { 0x10 } else { -8 };
            // Both players use record 0 of `D_80056CE0`.
            if let Some(l) = self.title.stock_labels.first() {
                put(&mut out, l, x, hud::VS_STOCK_Y, 128, None, NORMAL);
            }
            if let Some(d) = self.layout.digits[2].get(stocks[p] as usize) {
                put(&mut out, d, x + dx, hud::VS_STOCK_Y + 8, 128, Some(3), NORMAL);
            }
        }
        if repick {
            self.repick(&mut out, t, m.stage(), &page);
        } else {
            self.sd_objects(&mut out, m.stage(), None, &page);
        }
        // Layer 0, added before the SD objects and the HUD: a pending bomb's cut-in.
        if playing {
            for (f, x, y, s) in Self::cut_ins(m.bank(), &[&p1, &p2], true) {
                put(&mut out, &f, x, y, s, None, NORMAL);
            }
        }
        // Layer 0, added before all of those (func_8003BCD0 runs first in every sub-state).
        let spinning = matches!(sub, MatchSub::Play | MatchSub::Paused);
        Self::vs_strip(&mut out, sprites.bank(), &t.roulette, &t.tallies, spinning, &page);
        // Sub-state 3 adds the pause menu (func_8002E59C) before the strip and the HUD: in
        // front of everything.
        if sub == MatchSub::Paused {
            self.pause_menu(&mut out, &t.pause, &put);
        }
        out
    }

    /// Disc paths of every vs CPU stage background (the page decodes them up front).
    pub fn vs_cpu_backgrounds(&self) -> Vec<String> {
        let mut v: Vec<String> = (0..14).map(|s| hud::vs_cpu_background(s).to_string()).collect();
        v.dedup();
        v.sort();
        v.dedup();
        v
    }

    /// Disc path of vs CPU's background for its current stage (`func_80017EB0`).
    pub fn vs_cpu_background(&self, m: &CpuMatch) -> String {
        hud::vs_cpu_background(usize::from(m.inner().stage)).into()
    }

    /// The vs CPU screen (mode 13) over [`Screen::vs_cpu_background`]: the set match's fields,
    /// frame and strip with the stage's art swapped in, the human's score, the stage-won results
    /// (rank, clear time, bonus), the pause menu and the SD scripts.
    pub fn vs_cpu(&self, sprites: &mut Sprites, m: &mut CpuMatch) -> Vec<i32> {
        use stackers_core::vs_cpu::{Sub as CpuSub, RANKS};
        let vnao = m.stage().vnao;
        let stage = usize::from(m.inner().stage.min(13));
        let page = move |p: i16| vs_page_on(Some(stage), vnao, hud::vs_cpu_page(stage, p));
        let put = |out: &mut Vec<i32>, f: &Frame, x: i32, y: i32, s: i32, clut: Option<i16>, mode: i32| {
            let at = out.len();
            push(out, f, x, y, s, clut, mode);
            if out.len() > at {
                out[at] = page(f.page);
            }
        };
        if m.take_fresh() {
            sprites.reset();
        }
        let mut out = Vec::with_capacity(300 * SCREEN_STRIDE);
        let sub = m.inner().sub;
        let p1: Game = m.inner().versus.games[0].clone();
        let p2: Game = m.inner().versus.games[1].clone();
        let playing = sub == CpuSub::Play;
        let cut_ins = if playing { Self::cut_ins(m.bank(), &[&p1, &p2], false) } else { Vec::new() };
        let p2s = m.p2_sprites(sprites);
        self.vs_fields(&mut out, &put, sprites, p2s, [&p1, &p2], cut_ins, playing);

        // Layer 0: the score (func_8001DEF0).
        let t = m.inner();
        let (sx, sy) = hud::VS_CPU_SCORE_AT;
        self.number(&mut out, t.versus.games[t.me].hud.score, 8, sx, sy, 1, hud::VS_CPU_SCORE_CLUT, true);
        self.sd_objects(&mut out, m.stage(), None, &page);
        if playing {
            for (f, x, y, s) in Self::cut_ins(m.bank(), &[&p1, &p2], true) {
                put(&mut out, &f, x, y, s, None, NORMAL);
            }
        }
        let t = m.inner();
        // The results are layer 0, over the panel (SD layer 1).
        if t.results_shown() {
            let x = hud::VS_CPU_RESULT_X[t.me];
            let b = &t.bonus;
            self.text(&mut out, RANKS[b.rank()], x, hud::VS_CPU_RANK_Y, 1);
            let f = b.frames;
            let (min, sec, cs) = (f / 3600, f % 3600 / 60, f % 60 * 100 / 60);
            let ty = hud::VS_CPU_TIME_DIGITS_Y;
            let mut pen = self.text(&mut out, " ", x, hud::VS_CPU_TIME_Y, 1);
            pen = self.number(&mut out, min, 2, pen, ty, 1, 1, false);
            pen = self.glyph(&mut out, &self.layout.glyph_s, pen, hud::VS_CPU_TIME_Y, 1);
            pen = self.number(&mut out, sec, 2, pen, ty, 1, 1, true);
            pen = self.glyph(&mut out, &self.layout.glyph_d, pen, hud::VS_CPU_TIME_Y, 1);
            self.number(&mut out, cs, 2, pen, ty, 1, 1, true);
            self.number(&mut out, b.points, 9, x, hud::VS_CPU_BONUS_Y, 1, 1, false);
        }
        let spinning = matches!(sub, CpuSub::Play | CpuSub::Paused);
        Self::vs_strip(&mut out, sprites.bank(), &t.roulette, &t.tallies, spinning, &page);
        // The CPU's mascot (func_8003121C, layer 0, added before the strip, frame and score:
        // in front of them).
        if let Some(actor) = m.mascot() {
            Self::actor(&mut out, actor, &page, &|n| vs_page_on(Some(stage), [n, n], 26));
        }
        // Sub-state 3 adds the pause menu (func_8002E59C) before the mascot, the strip and the
        // score: in front of everything.
        if sub == CpuSub::Paused {
            self.pause_menu(&mut out, &m.inner().pause, &put);
        }
        out
    }

    /// Disc path of the attract demo's background (ST14A, vs CPU stage 6's).
    pub fn demo_background(&self) -> String {
        hud::vs_cpu_background(hud::DEMO_STAGE).into()
    }

    /// The attract demo (mode 2) for this frame, over [`Screen::demo_background`]: both fields,
    /// queues, arm spans and the two-player frame (no HUD), then in the panel sub-state the
    /// explanation panel (SD draw mode 4, semi-transparent), and the strip on top.
    /// `sprites` is player 1's (reset it when a set starts).
    pub fn demo(&self, sprites: &mut Sprites, d: &mut Demo) -> Vec<i32> {
        use stackers_core::demo::Sub as DemoSub;
        let vnao = d.stage().vnao;
        let page = move |p: i16| vs_page(vnao, hud::demo_page(p));
        let put = |out: &mut Vec<i32>, f: &Frame, x: i32, y: i32, s: i32, clut: Option<i16>, mode: i32| {
            let at = out.len();
            push(out, f, x, y, s, clut, mode);
            if out.len() > at {
                out[at] = page(f.page);
            }
        };
        if d.take_fresh() {
            sprites.reset();
        }
        let mut out = Vec::with_capacity(300 * SCREEN_STRIDE);
        let sub = d.inner().sub;
        let p1: Game = d.inner().versus.games[0].clone();
        let p2: Game = d.inner().versus.games[1].clone();
        let playing = sub == DemoSub::Play;
        // func_80038A08 runs in the play and panel sub-states.
        let cut_ins_on = matches!(sub, DemoSub::Play | DemoSub::Panel);
        let cut_ins = if cut_ins_on { Self::cut_ins(d.bank(), &[&p1, &p2], false) } else { Vec::new() };
        let p2s = d.p2_sprites(sprites);
        self.vs_fields(&mut out, &put, sprites, p2s, [&p1, &p2], cut_ins, playing);
        // Layer 0, back to front: a pending bomb's cut-in, the panel (func_8002EBA0), then the
        // strip (func_8003BCD0 adds first, so it ends up in front).
        if cut_ins_on {
            for (f, x, y, s) in Self::cut_ins(d.bank(), &[&p1, &p2], true) {
                put(&mut out, &f, x, y, s, None, NORMAL);
            }
        }
        if sub == DemoSub::Panel {
            for o in d.stage().visible() {
                let f = o.frame.unwrap();
                if f.w <= 0 || f.h <= 0 {
                    continue;
                }
                let (x, y, dw, dh, flip) = sdchr::panel_placement(o, &f);
                if dw <= 0 || dh <= 0 {
                    continue;
                }
                out.extend_from_slice(&[
                    page(f.page),
                    i32::from(f.clut),
                    i32::from(f.u),
                    i32::from(f.v),
                    i32::from(f.w),
                    i32::from(f.h),
                    x,
                    y,
                    dw,
                    dh,
                    i32::from(flip),
                    NORMAL | SEMI,
                ]);
            }
        }
        let t = d.inner();
        Self::vs_strip(&mut out, sprites.bank(), &t.roulette, &t.tallies, playing, &page);
        out
    }

    /// Layers 2 and 1 of the two-player screens: the cut-ins, player 2's field, player 1's,
    /// the queues, the arm spans, then the frame.
    fn vs_fields(
        &self,
        out: &mut Vec<i32>,
        put: &Put,
        sprites: &mut Sprites,
        p2s: &mut Sprites,
        games: [&Game; 2],
        cut_ins: Vec<(Frame, i32, i32, i32)>,
        playing: bool,
    ) {
        let (ox, oy) = hud::FIELD_ORIGIN;
        let [p1, p2] = games;
        // Layer 2, latest added first: the cut-ins, player 2's field, player 1's, the queues.
        for (f, x, y, s) in cut_ins {
            put(out, &f, x, y, s, None, NORMAL);
        }
        let f2 = p2s.field_frames(p2);
        let s2 = p2s.span_frames(p2);
        let q2 = p2s.queue_frames(p2);
        for (f, x, y, _) in f2 {
            put(out, &f, x + ox + hud::P2_FIELD_DX, y + oy, 128, None, NORMAL);
        }
        for (f, x, y, _) in sprites.field_frames(p1) {
            put(out, &f, x + ox, y + oy, 128, None, NORMAL);
        }
        let q1 = sprites.queue_frames(p1);
        for (pi, (g, frames)) in [(p1, q1), (p2, q2)].into_iter().enumerate() {
            for (slot, (&y, halves)) in g.queue_y.iter().zip(frames).enumerate() {
                let scale = hud::queue_scale(y);
                let cell = scale / 8;
                let x = hud::VS_QUEUE_X[pi] + (16 - cell) / 2;
                for (f, dy) in halves.iter().zip([0, cell]) {
                    if let Some(f) = f {
                        put(out, f, x, y + hud::QUEUE_TOP + dy, scale, None, NORMAL);
                    }
                }
                if slot == 0 {
                    let (px, py) = hud::QUEUE_PATCH_AT;
                    put(out, &self.layout.queue_patch, px, py, 128, None, hud::QUEUE_PATCH_BRIGHTNESS);
                }
            }
        }
        // Layer 1: the arm spans, then the frame.
        for (dx, list) in [(0, sprites.span_frames(p1)), (hud::P2_FIELD_DX, s2)] {
            for (f, x, y, part) in list.iter().rev() {
                let Part::Span { clut } = *part else { continue };
                put(out, f, x + ox + dx, y + oy, 128, Some(clut), NORMAL);
            }
        }
        self.vs_frame(out, put);
        // Layer 1, added by the score step before the frame: the all-clear sprites on top.
        if playing {
            Self::all_clears(out, put, sprites, games);
        }
    }

    /// `func_8003ACC0` → `func_8003AD34`: each player's all-clear sprite over its field
    /// (player 2 first, so player 1's ends up in front).
    fn all_clears(out: &mut Vec<i32>, put: &Put, sprites: &Sprites, games: [&Game; 2]) {
        for p in (0..2).rev() {
            let Some(a) = games[p].all_clear.filter(|a| a.steps > 0) else { continue };
            if let Some(f) = sprites.all_clear_frame(a) {
                let (x, y) = hud::ALL_CLEAR_AT[p];
                put(out, &f, x, y, 128, None, NORMAL);
            }
        }
    }

    /// `func_8001A82C`'s two-player frame (layer 1).
    fn vs_frame(&self, out: &mut Vec<i32>, put: &Put) {
        let mut right = self.layout.window_right;
        right.flip ^= 1;
        put(out, &right, hud::VS_WINDOW_RIGHT_X, 0, 128, None, NORMAL);
        put(out, &self.layout.window, 0, 0, 128, None, NORMAL);
    }

    /// Disc path of the title background's tile map (`DATA/DOMI_BG.OBJ`).
    pub fn title_bg_file(&self) -> String {
        title::BG_FILE.into()
    }

    /// Give the screen `DATA/DOMI_BG.OBJ` so [`Screen::title`] can draw the tiled background.
    pub fn set_title_bg(&mut self, data: &[u8]) -> Result<(), JsError> {
        self.bg = Some(BgMap::parse(data, 0).map_err(crate::js_err)?);
        Ok(())
    }

    /// The title / menu screen for this frame, back to front (black behind it).
    pub fn title(&self, fe: &FrontEnd) -> Vec<i32> {
        let f = fe.front();
        let stage = fe.stage();
        let page = |p: i16| -> i32 {
            match p {
                26 => title::vnao_page(stage.vnao[0]),
                27 => title::vnao_page(stage.vnao[1]),
                _ => title::PAGE_BASE + i32::from(p),
            }
        };
        let mut out = Vec::with_capacity(64 * SCREEN_STRIDE);
        let put = |out: &mut Vec<i32>, fr: &Frame, x, y, scale, clut, mode| {
            let mut fr = *fr;
            let p = page(fr.page);
            fr.page = 0;
            let at = out.len();
            push(out, &fr, x, y, scale, clut, mode);
            if out.len() > at {
                out[at] = p;
            }
        };

        // Tiled background (`func_800418CC`, its own ordering table: under every sprite).
        if let (true, Some(bg)) = (f.bg, &self.bg) {
            for (u, v, x, y) in bg.tiles_at(f.bg_scroll) {
                out.extend_from_slice(&[title::PAPER_PAGE, 0, u, v, bg.cell_w, bg.cell_h, x, y, bg.cell_w, bg.cell_h, 0, NORMAL]);
            }
        }

        // Menu description (layer 2, under the SD objects).
        if let Some(d) = f.menu.and_then(|m| m.description).and_then(|i| self.title.descriptions.get(i)) {
            put(&mut out, d, DESCRIPTION_AT.0, DESCRIPTION_AT.1, 128, None, NORMAL);
        }

        // TECMO logo: func_8001C03C(frame, 0, −20, …, sx 128, sy) about the screen centre.
        if let Some(Splash::Tecmo { scale_y }) = f.splash {
            let t = &self.title.tecmo;
            let (w, h) = (i32::from(t.w), i32::from(t.h));
            let x0 = 160 - ((w * (128 + 7)) >> 7) / 2;
            let y0 = 120 - 20 - ((h * (scale_y + 7)) >> 7) / 2;
            let dh = (h * scale_y) >> 7;
            if dh > 0 {
                out.extend_from_slice(&[
                    page(t.page),
                    i32::from(t.clut),
                    i32::from(t.u),
                    i32::from(t.v),
                    w,
                    h,
                    x0,
                    y0,
                    w,
                    dh,
                    0,
                    NORMAL,
                ]);
            }
        }

        // Option volume bars (`func_8003ED18`, layer 1): the pale bars are added last, so drawn
        // first, under the blue ones.
        if let Some(OptionDraw::Sound { se, cd }) = f.option {
            for y in option::BAR_Y {
                let w = option::BAR_W;
                out.extend_from_slice(&[FILL_PAGE, 0x8080FF, 0, 0, w, option::BAR_H, option::BAR_X, y, w, option::BAR_H, 0, NORMAL | SEMI]);
            }
            for (y, v) in option::BAR_Y.into_iter().zip([se, cd]) {
                let w = i32::from(v);
                if w > 0 {
                    out.extend_from_slice(&[FILL_PAGE, 0x0000FF, 0, 0, w, option::BAR_H, option::BAR_X, y, w, option::BAR_H, 0, NORMAL]);
                }
            }
        }

        // SD_CHR objects, layer 0: the first slot added is drawn last (on top).
        if let Some(run) = f.sd {
            let bright = if run.mode == 1 { i32::from(f.brightness) } else { NORMAL };
            for (slot, o) in stage.objects.iter().enumerate().rev() {
                let Some(fr) = o.frame.filter(|_| o.visible) else { continue };
                if fr.w <= 0 || fr.h <= 0 || bright == 0 {
                    continue;
                }
                let (x, y, dw, dh, flip) = sdchr::placement(o, &fr);
                let clut = if slot < SLOTS { f.palettes[slot] } else { 0 };
                out.extend_from_slice(&[
                    page(fr.page),
                    i32::from(clut),
                    i32::from(fr.u),
                    i32::from(fr.v),
                    i32::from(fr.w),
                    i32::from(fr.h),
                    x,
                    y,
                    dw,
                    dh,
                    i32::from(flip),
                    bright,
                ]);
            }
        }

        // Arrows and the Arcade overlays: func_8001B860 / func_8001B148 on layer 0, added
        // before the VM runs, so in front of the SD objects; the first added is on top.
        let mut arrows: Vec<Arrow> = f.menu.map(|m| Arrow { frame: m.arrow_frame, at: m.arrow, flip: false }).into_iter().collect();
        arrows.extend_from_slice(&f.arrows);
        for a in arrows.iter().rev() {
            if let Some(fr) = self.title.arrows.get(a.frame) {
                let mut fr = *fr;
                fr.flip ^= i16::from(a.flip);
                put(&mut out, &fr, a.at.0, a.at.1, 128, Some(0), NORMAL);
            }
        }
        for o in f.overlays.iter().rev() {
            match *o {
                Overlay::Sets { choice, at } => {
                    if let Some(fr) = self.title.set_digits.get(choice) {
                        put(&mut out, fr, at.0, at.1, 128, None, NORMAL);
                    }
                }
                Overlay::Stock { player, value, x } => {
                    // func_8001D778 adds the digit first, then the label: the digit is on top.
                    let dx = if player == 0 { 0x10 } else { -8 };
                    put(&mut out, &self.title.stock_labels[0], x, STOCK_Y, 128, None, NORMAL);
                    if let Some(fr) = self.title.small_digits.get(usize::from(value)) {
                        put(&mut out, fr, x + dx, STOCK_Y + 8, 128, Some(3), NORMAL);
                    }
                }
            }
        }
        // Option controller page: the button labels and the layout numbers (OPTION page,
        // tpage (384, 0) set by `func_8003F140`'s DR_MODE).
        if let Some(OptionDraw::Pads { layout, number_y }) = f.option {
            let opt = title::PAGE_BASE + title::OPTION_SLOT;
            for (p, &l) in layout.iter().enumerate() {
                let l = usize::from(l.min(option::LAYOUTS - 1));
                for (i, &(u, v, w, h)) in option::LABEL_UVWH.iter().enumerate() {
                    let (u, v, w, h) = (i32::from(u), i32::from(v), i32::from(w), i32::from(h));
                    let (x, y) = (option::label_x(p, i), option::LABEL_Y[l][i]);
                    out.extend_from_slice(&[opt, i32::from(option::LABEL_CLUT), u, v, w, h, x, y, w, h, 0, NORMAL]);
                }
            }
            for (p, &l) in layout.iter().enumerate() {
                let u = 0x40 + 16 * i32::from(l);
                out.extend_from_slice(&[opt, 0, u, 0x30, 16, 16, option::NUMBER_X[p], number_y, 16, 16, 0, NORMAL]);
            }
        }
        out
    }

    /// The spinning title logo of title state 1, drawn after [`Screen::title`]'s list:
    /// `[page, clut, u, v, w, h, centre_x, centre_y, scale (4096 = 1), angle (4096 = 1°)]`,
    /// or empty.
    pub fn title_spin(&self, fe: &FrontEnd) -> Vec<i32> {
        let Some(Splash::Spin { scale, rotate }) = fe.front().splash else { return Vec::new() };
        let l = &self.title.logo;
        vec![
            title::PAGE_BASE + i32::from(l.page),
            0,
            i32::from(l.u),
            i32::from(l.v),
            i32::from(l.w),
            i32::from(l.h),
            160,
            120,
            scale,
            rotate,
        ]
    }

    /// The solo screen for this frame, back to front, in screen pixels.
    pub fn solo(&self, sprites: &mut Sprites, session: &Session) -> Vec<i32> {
        let mut out = Vec::with_capacity(160 * SCREEN_STRIDE);
        let (ox, oy) = hud::FIELD_ORIGIN;

        // Field blobs and the controlled pair (`func_800195FC`, scale 128).
        for s in sprites.field(session).chunks_exact(crate::sprites::SPRITE_STRIDE) {
            let [page, clut, u, v, w, h, x, y, flip] = s.try_into().unwrap();
            out.extend_from_slice(&[page, clut, u, v, w, h, x + ox, y + oy, w, h, flip, NORMAL]);
        }

        // Queue window (`func_80023294`): satellite on top, pivot one scaled cell below.
        let frames = sprites.queue_frames(session.game());
        for (&y, halves) in session.game().queue_y.iter().zip(frames) {
            let scale = hud::queue_scale(y);
            let cell = scale / 8;
            let x = hud::QUEUE_X + (16 - cell) / 2;
            for (f, dy) in halves.iter().zip([0, cell]) {
                if let Some(f) = f {
                    push(&mut out, f, x, y + hud::QUEUE_TOP + dy, scale, None, NORMAL);
                }
            }
        }
        let (px, py) = hud::QUEUE_PATCH_AT;
        push(&mut out, &self.layout.queue_patch, px, py, 128, None, hud::QUEUE_PATCH_BRIGHTNESS);

        // HUD panels (`func_8001A560`): the monitor is drawn first, the label over it.
        for i in 0..3 {
            let clut = Some(hud::SOLO_PANEL_CLUTS[i]);
            let y = hud::PANEL_Y[i];
            push(&mut out, &self.layout.panels[0], hud::PANEL_X, y, 128, clut, NORMAL | SEMI);
            let label = &self.layout.panels[hud::SOLO_PANEL_LABELS[i]];
            push(&mut out, label, hud::PANEL_X, y, 128, clut, NORMAL);
        }

        // Arm spans (`func_800258A8`, OT layer 1, added after the frame: drawn under it).
        for (f, x, y, part) in sprites.span_frames(session.game()).iter().rev() {
            let Part::Span { clut } = *part else { continue };
            push(&mut out, f, x + ox, y + oy, 128, Some(clut), NORMAL);
        }

        // Window frame and next-pair window (`func_8001A7BC`, OT layer 1: over the field).
        push(&mut out, &self.layout.window, 0, 0, 128, None, NORMAL);

        // Layer 0: the mascot (`func_80031590`, added after the counters: behind them).
        if let Some(stage) = session.mascot() {
            Self::actor(&mut out, stage, &|p| i32::from(p), &title::vnao_page);
        }

        // HUD numbers, OT layer 0 (in front of the frame).
        let h = session.game().hud;
        let (x, y) = hud::SCORE_AT;
        self.number(&mut out, h.score, hud::SCORE_DIGITS, x, y, 1, hud::HUD_CLUT, true);
        for (value, record) in [(h.erased, false), (self.record, true)] {
            if let Some((font, digits, x, y, clut, zeros)) = hud::counter_style(value, record) {
                self.number(&mut out, value, digits, x, y, font, clut, zeros);
            }
        }
        let (hours, minutes, seconds) = h.hms();
        let (x, y) = hud::TIME_AT;
        let clut = hud::HUD_CLUT;
        let x = self.number(&mut out, hours, 2, x + self.layout.space, y, 1, clut, false);
        let x = self.glyph(&mut out, &self.layout.glyph_h, x, y, clut);
        let x = self.number(&mut out, minutes, 2, x, y, 1, clut, true);
        let x = self.glyph(&mut out, &self.layout.glyph_s, x, y, clut);
        self.number(&mut out, seconds, 2, x, y, 1, clut, true);

        self.overlays(&mut out, session);
        out
    }
    /// The Chain Reaction screen (mode 7) for this frame, back to front. The page draws the
    /// title's paper background ([`Screen::chain_background`]) under it.
    pub fn chain(&self, sprites: &mut Sprites, session: &Session) -> Vec<i32> {
        let game = session.game();
        let cam = game.camera.y;
        let scale = i32::from(game.camera.scale);
        let page = |p: i16| -> i32 {
            if chain::PAGES.contains(&p) {
                CHAIN_PAGE_BASE + i32::from(p)
            } else {
                i32::from(p)
            }
        };
        let put = |out: &mut Vec<i32>, f: &Frame, x: i32, y: i32, s: i32, clut: Option<i16>, mode: i32| {
            let at = out.len();
            push(out, f, x, y, s, clut, mode);
            if out.len() > at {
                out[at] = page(f.page);
            }
        };
        let mut out = Vec::with_capacity(200 * SCREEN_STRIDE);
        let c = &self.chain;

        // Layer 3: the tower (`func_8001A2B4`), zoomed one step wider than the field.
        let segment = |i: usize| {
            let (x, y) = chain::tower_segment(i, cam);
            chain::zoom(x, y, scale, cam)
        };
        for i in (0..chain::TOWER_SEGMENTS).rev() {
            let (x, y) = segment(i);
            if chain::tower_on_screen(y) {
                put(&mut out, &c.tower[i], x, y, scale + 1, None, NORMAL);
            }
        }

        // Layer 2, back to front: panel monitors, labels (`func_8001A560`), the field's blobs,
        // then what `func_80019998` adds for the pair, and the queue in front.
        for i in 0..3 {
            let clut = Some(chain::PANEL_CLUTS[i]);
            put(&mut out, &self.layout.panels[0], hud::PANEL_X, hud::PANEL_Y[i], 128, clut, NORMAL);
        }
        for i in (0..3).rev() {
            let label = &self.layout.panels[chain::PANEL_LABELS[i]];
            put(&mut out, label, hud::PANEL_X, hud::PANEL_Y[i], 128, Some(chain::PANEL_CLUTS[i]), NORMAL);
        }
        let frames = sprites.field_frames(session.game());
        let zoomed = |x: i32, y: i32| {
            let (x, y) = chain::field_to_screen(x, y, cam);
            chain::zoom(x, y, scale, cam)
        };
        for (f, x, y, _) in frames.iter().rev().filter(|e| e.3 == Part::Cell) {
            let (x, y) = zoomed(*x, *y);
            if chain::on_screen(y) {
                put(&mut out, f, x, y, scale, None, NORMAL);
            }
        }
        if game.piece_active() {
            let d = game.pair_draw();
            let pivot = frames.iter().find(|e| e.3 == Part::Pivot).map(|e| e.0);
            let satellite = frames.iter().find(|e| e.3 == Part::Satellite).map(|e| e.0);
            let (px, py) = zoomed(d.pivot.0, d.pivot.1);
            let visible = chain::on_screen(py);
            let arrow = chain::arrow_frame(game.play_frames());
            let (ax, ay) = chain::PANEL_PIVOT;
            // Added last → drawn first.
            if let Some(f) = &satellite {
                let (dx, dy) = d.panel_satellite;
                put(&mut out, f, ax + dx, ay + dy, 128, None, NORMAL);
                if visible {
                    let (sx, sy) = zoomed(d.satellite.0, d.satellite.1);
                    put(&mut out, f, sx, sy, scale, None, NORMAL);
                }
            }
            let my = chain::marker_y(game.player.y);
            put(&mut out, &c.marker[arrow], chain::MARKER_X, my, chain::SMALL, None, NORMAL);
            if let Some(f) = &pivot {
                put(&mut out, f, ax, ay, 128, None, NORMAL);
            }
            if visible {
                if let Some(f) = &pivot {
                    put(&mut out, f, px, py, scale, None, NORMAL);
                }
            } else {
                let (x, y) = chain::PANEL_ARROW;
                put(&mut out, &c.arrow_up[arrow], x, y, chain::SMALL, None, NORMAL);
                if py >= 0xF0 {
                    put(&mut out, &c.arrow_down[arrow], px, chain::ARROW_BOTTOM_Y, scale, None, NORMAL);
                } else {
                    put(&mut out, &c.arrow_up[arrow], px, chain::ARROW_TOP_Y, scale, None, NORMAL);
                }
            }
        }
        // `func_80023294` skips the solo patch under the window in mode 7.
        let qframes = sprites.queue_frames(session.game());
        for (&y, halves) in game.queue_y.iter().zip(qframes).rev() {
            let s = hud::queue_scale(y);
            let cell = s / 8;
            let x = chain::QUEUE_X + (16 - cell) / 2;
            for (f, dy) in halves.iter().zip([0, cell]) {
                if let Some(f) = f {
                    put(&mut out, f, x, y + hud::QUEUE_TOP + dy, s, None, NORMAL);
                }
            }
        }

        // Layer 1: the next-pair window, the tower's top edge, the chain counters.
        put(&mut out, &c.window, chain::WINDOW_AT.0, chain::WINDOW_AT.1, 128, None, NORMAL);
        let (x0, y0) = segment(0);
        if chain::tower_on_screen(y0) {
            put(&mut out, &c.tower[chain::TOWER_SEGMENTS], x0, y0, scale + 1, None, NORMAL);
        }
        for (value, y) in [(game.chain_len, chain::COUNTER_Y[0]), (game.record, chain::COUNTER_Y[1])] {
            let digits = [value / 100 % 10, value / 10 % 10, value % 10];
            for (d, x) in digits.into_iter().zip(chain::COUNTER_X) {
                put(&mut out, &c.digits[d as usize], x, y, 128, None, NORMAL);
            }
        }

        // Arm spans (`func_800258A8`): layer 1, added before the window and counters, so
        // drawn over them.
        for (f, x, y, part) in sprites.span_frames(session.game()).iter().rev() {
            let Part::Span { clut } = *part else { continue };
            let (x, y) = zoomed(*x, *y);
            if chain::on_screen(y) {
                put(&mut out, f, x, y, scale, Some(clut), NORMAL);
            }
        }

        // Layer 0: the mascot (`func_80032934`, added before the score), then the score.
        if let Some(stage) = session.mascot() {
            Self::actor(&mut out, stage, &page, &title::vnao_page);
        }
        let (x, y) = chain::SCORE_AT;
        self.number(&mut out, game.hud.score, hud::SCORE_DIGITS, x, y, 1, hud::HUD_CLUT, true);
        self.overlays(&mut out, session);
        out
    }

    /// The paper tiles behind Chain Reaction (`DOMI_BG` set 0, unscrolled; needs
    /// [`Screen::set_title_bg`]): `[page, clut, u, v, w, h, x, y, w, h, 0, 128]` each.
    pub fn chain_background(&self) -> Vec<i32> {
        let mut out = Vec::new();
        if let Some(bg) = &self.bg {
            for (u, v, x, y) in bg.tiles_at((0, 0)) {
                out.extend_from_slice(&[title::PAPER_PAGE, 0, u, v, bg.cell_w, bg.cell_h, x, y, bg.cell_w, bg.cell_h, 0, NORMAL]);
            }
        }
        out
    }
}

impl Screen {
    /// The pause menu and the game-over scripts, in front of everything else.
    fn overlays(&self, out: &mut Vec<i32>, session: &Session) {
        self.pause_menu(out, &session.game().pause, &push);
        self.sd_objects(out, session.stage(), None, &|p| i32::from(p));
    }

    /// The pause menu (`func_8002DE14`; modes 13–15 `func_8002E59C`, told apart by the item
    /// count), added before the HUD numbers: in front of them.
    fn pause_menu(&self, out: &mut Vec<i32>, pause: &stackers_core::Pause, put: &Put) {
        if !pause.active {
            return;
        }
        let versus = pause.items == 2;
        let (items, confirm, frame): (&[(usize, i32, i32)], _, &[(usize, i32, i32)]) = if versus {
            (&hud::VS_PAUSE_ITEMS, (hud::VS_PAUSE_CONFIRM, hud::VS_PAUSE_CONFIRM_CLUT), &hud::VS_PAUSE_FRAME)
        } else {
            (&hud::PAUSE_ITEMS, (hud::PAUSE_CONFIRM, hud::PAUSE_CONFIRM_CLUT), &hud::PAUSE_FRAME)
        };
        // The header (and the solo menu's separate monitor) take the header palette.
        let (header, headers) = if versus { (hud::VS_PAUSE_HEADER_CLUT, 1) } else { (hud::PAUSE_HEADER_CLUT, 2) };
        let mut add = Vec::new();
        if matches!(pause.menu, menu::QUIT | menu::CONFIRM) {
            let ((r, x, y), clut) = confirm;
            add.push((r, x, y, clut, BRIGHT_ON));
        } else {
            let n = usize::from(pause.items);
            for (i, &(r, x, y)) in items.iter().enumerate() {
                add.push(if i < n {
                    (r, x, y, pause.item_clut[i], pause.item_bright[i])
                } else {
                    (r, x, y, hud::PAUSE_LEGEND_CLUT, BRIGHT_ON)
                });
            }
        }
        for (i, &(r, x, y)) in frame.iter().enumerate() {
            let clut = if i < headers { header } else { hud::PAUSE_LEGEND_CLUT };
            add.push((r, x, y, clut, BRIGHT_ON));
        }
        // First added is drawn last.
        for &(r, x, y, clut, bright) in add.iter().rev() {
            put(out, &self.layout.panels[r], x, y, 128, Some(clut), i32::from(bright));
        }
    }

    /// The two-player cut-ins (`func_80038A08`) on one OT layer, back to front: per player
    /// (player 2 first, since player 1's are added first) the burst, the backdrop, then the
    /// text, as `(frame, x, y, scale)`. They are on layer 2 (added after everything else
    /// there, so behind the fields) unless that player's bomb is pending, which puts them on
    /// layer 0 in front of everything.
    fn cut_ins(bank: Option<&SdBank>, games: &[&Game], front: bool) -> Vec<(Frame, i32, i32, i32)> {
        let mut out = Vec::new();
        let Some(bank) = bank else { return out };
        for (p, g) in games.iter().enumerate().rev() {
            let Some(c) = g.cut_in else { continue };
            if (g.bomb != 0) != front {
                continue;
            }
            for obj in (0..3).rev() {
                if let Some(f) = bank.cut_in_frame(obj, c.entry, u32::from(c.age)) {
                    let (x, y) = sdchr::CUT_IN_AT[p][obj];
                    out.push((f, x, y, sdchr::CUT_IN_SCALE[obj]));
                }
            }
        }
        out
    }

    /// SD_CHR objects, OT layer 0 and queued before the HUD numbers, so they end up in front of
    /// them: draw mode 2 (`func_80030070`, the frame's palette) or, with `palettes`, draw mode 1
    /// (each slot's palette). Draw mode 1 puts the first slot last (on top), as the title does;
    /// draw mode 2 keeps the order the solo game-over has always used.
    /// `func_8003C7C4`: the strip over both fields (slots, garbage digits, the special-column
    /// arrow and its sparkle, and the message while it spins), all layer 0.
    fn vs_strip(
        out: &mut Vec<i32>,
        bank: &AnimBank,
        roulette: &[Roulette; 2],
        tallies: &[[ColumnTally; 6]; 2],
        spinning_shown: bool,
        page: &dyn Fn(i16) -> i32,
    ) {
        use stackers_formats::hud::strip;
        // A script's frame after `steps` steps (loops wrap; 0 steps: nothing yet).
        let replay = |script: Option<u32>, steps: u16| -> Option<Frame> {
            let script = script?;
            if steps == 0 {
                return None;
            }
            let steps = match bank.loop_period(script) {
                Some(p) => (u32::from(steps) - 1) % p + 1,
                None => u32::from(steps.min(300)),
            };
            let mut player = ScriptPlayer::new(script);
            for _ in 0..steps {
                player.step(bank);
            }
            bank.frame(player.frame)
        };
        // func_8001C03C: centred on screen-centre offsets, scaled sx/sy, the palette given.
        let centred = |f: &Frame, x: i32, y: i32, sx: i32, sy: i32, clut: i16| -> Option<[i32; SCREEN_STRIDE]> {
            let (w, h) = (i32::from(f.w), i32::from(f.h));
            let (x0, y0, dw, dh) = strip::centred(w, h, x, y, sx, sy);
            (dw > 0 && dh > 0).then(|| {
                [page(f.page), i32::from(clut), i32::from(f.u), i32::from(f.v), w, h, x0, y0, dw, dh, i32::from(f.flip != 0), NORMAL]
            })
        };
        // func_8001B4D0: as func_8001B148 with the palette given.
        let plain = |f: &Frame, x: i32, y: i32, clut: i16| -> Option<[i32; SCREEN_STRIDE]> {
            let mut v = Vec::new();
            push(&mut v, f, x, y, 128, Some(clut), NORMAL);
            let mut a: [i32; SCREEN_STRIDE] = v.try_into().ok()?;
            a[0] = page(f.page);
            Some(a)
        };
        // In the order the original adds them (so the first is on top).
        let mut added: Vec<Option<[i32; SCREEN_STRIDE]>> = Vec::new();
        for (p, r) in roulette.iter().enumerate() {
            if spinning_shown && r.script == 1 {
                let f = replay(bank.table_script(HUD_MESSAGE, 1), r.timer);
                added.push(f.and_then(|f| plain(&f, strip::MESSAGE_X[p], strip::MESSAGE_Y, strip::MESSAGE_CLUT)));
            }
        }
        for (p, r) in roulette.iter().enumerate() {
            let x = strip::COLUMN_X[p][usize::from(r.column)] + strip::COLUMN_DX + i32::from(r.wobble);
            let f = replay(bank.table_script(HUD_SPARKLE, u32::from(r.script == 1)), r.timer);
            let s = strip::SPARKLE_SCALE;
            added.push(f.and_then(|f| centred(&f, x, strip::SPARKLE_Y, s, s, strip::SPARKLE_CLUT)));
        }
        for (p, r) in roulette.iter().enumerate() {
            let x = strip::COLUMN_X[p][usize::from(r.column)] + strip::COLUMN_DX;
            added.push(centred(&strip::ARROW, x, strip::ARROW_Y, 128, strip::ROW_SCALE, i16::from(r.clut)));
        }
        for x in strip::BAR_X {
            added.push(centred(&strip::BAR, x, strip::ROW_Y, 128, strip::ROW_SCALE, strip::BAR_CLUT));
        }
        for (p, cols) in tallies.iter().enumerate() {
            for (c, tally) in cols.iter().enumerate() {
                let (entry, steps) = tally.script();
                let f = replay(bank.table_script(HUD_DIGITS, u32::from(entry)), steps);
                let x = strip::DIGIT_X[p] + 16 * c as i32;
                added.push(f.and_then(|f| plain(&f, x, strip::DIGIT_Y, i16::from(tally.clut))));
            }
        }
        for x in strip::SLOT_X {
            for c in 0..6 {
                added.push(centred(&strip::SLOT, x + 16 * c, strip::ROW_Y, 128, strip::ROW_SCALE, strip::SLOT_CLUT));
            }
        }
        for s in added.iter().rev().flatten() {
            out.extend_from_slice(s);
        }
    }

    /// The set match's re-pick (`func_8002B8EC`), all layer 0: the difficulty lists (SD scripts
    /// 0x42 / 0x43, draw mode 1 with the list's palettes), then on top of them each player's
    /// arrow (`D_80056A88`, LEVEL2, palette 3; player 2's mirrored; not while the lists come
    /// in) and, while picking, the □ stocks at the top (`func_8001D778(0x40, 0xF0, 0x30)`).
    fn repick(&self, out: &mut Vec<i32>, t: &SetMatch, stage: &SdStage, page: &dyn Fn(i16) -> i32) {
        use stackers_core::set_match::repick;
        let put = |out: &mut Vec<i32>, f: &Frame, x: i32, y: i32, clut: Option<i16>| {
            let at = out.len();
            push(out, f, x, y, 128, clut, NORMAL);
            if out.len() > at {
                out[at] = page(f.page);
            }
        };
        self.sd_objects(out, stage, Some(&t.repick_palettes), page);
        if t.phase < repick::PICK {
            return;
        }
        // Player 2's arrow is added second, so player 1's is on top.
        for p in [1, 0] {
            let r = &t.repick[p];
            if let Some(a) = self.layout.course_arrows.get(r.arrow_frame) {
                let mut a = *a;
                a.flip ^= i16::from(p == 1);
                put(out, &a, r.arrow.0, r.arrow.1, Some(3));
            }
        }
        if t.phase == repick::PICK {
            for p in [1, 0] {
                let x = STOCK_X[p];
                let dx = if p == 0 { 0x10 } else { -8 };
                put(out, &self.title.stock_labels[0], x, STOCK_Y, None);
                if let Some(d) = self.layout.digits[2].get(usize::from(t.bombs[p])) {
                    put(out, d, x + dx, STOCK_Y + 8, Some(3));
                }
            }
        }
    }

    /// An actor's objects (`func_80032934`): pages 26/27 are whichever RAM slot the actor
    /// uploaded there, resolved by `sheet` (VNAO1–3 outside vs CPU).
    fn actor(out: &mut Vec<i32>, stage: &SdStage, page: &dyn Fn(i16) -> i32, sheet: &dyn Fn(u8) -> i32) {
        for o in stage.visible() {
            let f = o.frame.unwrap();
            let (x, y, s) = sdchr::actor_placement(o, &f);
            let mut f = f;
            f.flip = i16::from(o.flip != i32::from(f.flip));
            let p = match f.page {
                26 => sheet(stage.vnao[0]),
                27 => sheet(stage.vnao[1]),
                p => page(p),
            };
            let at = out.len();
            push(out, &f, x, y, s, None, NORMAL);
            if out.len() > at {
                out[at] = p;
            }
        }
    }

    fn sd_objects(&self, out: &mut Vec<i32>, stage: &SdStage, palettes: Option<&[i16]>, page: &dyn Fn(i16) -> i32) {
        let mut order: Vec<usize> = (0..stage.objects.len()).collect();
        if palettes.is_some() {
            order.reverse();
        }
        for slot in order {
            let o = &stage.objects[slot];
            let Some(f) = o.frame.filter(|_| o.visible) else { continue };
            if f.w <= 0 || f.h <= 0 {
                continue;
            }
            let clut = palettes.map_or(f.clut, |p| p.get(slot).copied().unwrap_or(0));
            let (x, y, dw, dh, flip) = sdchr::placement(o, &f);
            out.extend_from_slice(&[
                page(f.page),
                i32::from(clut),
                i32::from(f.u),
                i32::from(f.v),
                i32::from(f.w),
                i32::from(f.h),
                x,
                y,
                dw,
                dh,
                i32::from(flip),
                NORMAL,
            ]);
        }
    }

    /// `func_8001D4D8(value, digits, x, y, clut, font, zeros)`; returns the pen x after it.
    #[allow(clippy::too_many_arguments)]
    fn number(
        &self,
        out: &mut Vec<i32>,
        value: u32,
        digits: usize,
        mut x: i32,
        y: i32,
        font: usize,
        clut: i16,
        zeros: bool,
    ) -> i32 {
        let glyphs = &self.layout.digits[font];
        let advance = i32::from(glyphs[0].w);
        for slot in hud::digit_slots(value, digits, zeros) {
            if let Some(d) = slot {
                push(out, &glyphs[usize::from(d)], x, y, 128, Some(clut), NORMAL);
            }
            x += advance;
        }
        x
    }

    /// `func_8001D29C` with the text font: A–Y are records `c − 'A'`, advancing by record 0's
    /// width; a space advances the same. Returns the pen x after it.
    fn text(&self, out: &mut Vec<i32>, s: &str, mut x: i32, y: i32, clut: i16) -> i32 {
        for c in s.bytes() {
            if let Some(f) = c.checked_sub(b'A').and_then(|i| self.layout.letters.get(usize::from(i))) {
                push(out, f, x, y, 128, Some(clut), NORMAL);
            }
            x += self.layout.space;
        }
        x
    }

    /// One `func_8001D29C` escape glyph; returns the pen x after it.
    fn glyph(&self, out: &mut Vec<i32>, f: &Frame, x: i32, y: i32, clut: i16) -> i32 {
        push(out, f, x, y, 128, Some(clut), NORMAL);
        x + i32::from(f.w)
    }
}
