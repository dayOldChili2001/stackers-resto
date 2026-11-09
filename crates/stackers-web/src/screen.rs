//! The whole 320×240 solo screen as one draw list: field, queue window, HUD panels and frame.
//!
//! The background ([`Screen::background`]) is a full-screen image the page draws first; this
//! list goes on top, back to front. The original sorts by OT layer (the frame is on layer 1,
//! everything else on layer 2, nothing here overlaps within a layer except as listed).

use crate::front::FrontEnd;
use crate::session::Session;
use crate::sprites::Sprites;
use stackers_core::front::{Splash, DESCRIPTION_AT, SLOTS};
use stackers_core::pause::{menu, BRIGHT_ON};
use crate::sprites::Part;
use stackers_formats::anm::Frame;
use stackers_formats::chain::{self, ChainScreen};
use stackers_formats::hud::{self, SoloScreen, Stage};
use stackers_formats::sdchr;
use stackers_formats::title::{self, BgMap, TitleLayout};
use wasm_bindgen::prelude::*;

/// Ints per sprite in [`Screen::solo`]:
/// `[page, clut, u, v, w, h, x, y, draw_w, draw_h, flip, mode]`, `mode` = brightness (128 =
/// unchanged) | `semi << 8` (STP texels blend 50/50).
pub const SCREEN_STRIDE: usize = 12;
const NORMAL: i32 = 128;
/// Page ids of the art Chain Reaction loads over solo slots ([`chain::PAGES`]) are this + slot.
pub const CHAIN_PAGE_BASE: i32 = 200;
const SEMI: i32 = 1 << 8;

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
        pages
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

        // Menu arrow (layer 3) and description (layer 2).
        if let Some(m) = f.menu {
            if let Some(a) = self.title.arrows.get(m.arrow_frame) {
                put(&mut out, a, m.arrow.0, m.arrow.1, 128, Some(0), NORMAL);
            }
            if let Some(d) = m.description.and_then(|i| self.title.descriptions.get(i)) {
                put(&mut out, d, DESCRIPTION_AT.0, DESCRIPTION_AT.1, 128, None, NORMAL);
            }
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
        let frames = sprites.queue_frames(session);
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
        for (f, x, y, part) in sprites.span_frames(session).iter().rev() {
            let Part::Span { clut } = *part else { continue };
            push(&mut out, f, x + ox, y + oy, 128, Some(clut), NORMAL);
        }

        // Window frame and next-pair window (`func_8001A7BC`, OT layer 1: over the field).
        push(&mut out, &self.layout.window, 0, 0, 128, None, NORMAL);

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
        let frames = sprites.field_frames(session);
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
        let qframes = sprites.queue_frames(session);
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
        for (f, x, y, part) in sprites.span_frames(session).iter().rev() {
            let Part::Span { clut } = *part else { continue };
            let (x, y) = zoomed(*x, *y);
            if chain::on_screen(y) {
                put(&mut out, f, x, y, scale, Some(clut), NORMAL);
            }
        }

        // Layer 0: the mascot (`func_80032934`, added before the score), then the score.
        if let Some(stage) = session.mascot() {
            for o in stage.visible() {
                let f = o.frame.unwrap();
                let (x, y, s) = sdchr::actor_placement(o, &f);
                let mut f = f;
                f.flip = i16::from(o.flip != i32::from(f.flip));
                let p = match f.page {
                    26 => title::vnao_page(stage.vnao[0]),
                    27 => title::vnao_page(stage.vnao[1]),
                    p => page(p),
                };
                let at = out.len();
                push(&mut out, &f, x, y, s, None, NORMAL);
                if out.len() > at {
                    out[at] = p;
                }
            }
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
        // Pause menu (`func_8002DE14`), added before the HUD numbers: in front of them.
        let pause = &session.game().pause;
        if pause.active {
            let mut add = Vec::new();
            let header = hud::PAUSE_HEADER_CLUT;
            if matches!(pause.menu, menu::QUIT | menu::CONFIRM) {
                let (r, x, y) = hud::PAUSE_CONFIRM;
                add.push((r, x, y, hud::PAUSE_CONFIRM_CLUT, BRIGHT_ON));
            } else {
                for (i, &(r, x, y)) in hud::PAUSE_ITEMS.iter().enumerate() {
                    add.push(match i {
                        0..=2 => (r, x, y, pause.item_clut[i], pause.item_bright[i]),
                        _ => (r, x, y, hud::PAUSE_LEGEND_CLUT, BRIGHT_ON),
                    });
                }
            }
            for (i, &(r, x, y)) in hud::PAUSE_FRAME.iter().enumerate() {
                let clut = if i < 2 { header } else { hud::PAUSE_LEGEND_CLUT };
                add.push((r, x, y, clut, BRIGHT_ON));
            }
            // First added is drawn last.
            for &(r, x, y, clut, bright) in add.iter().rev() {
                push(out, &self.layout.panels[r], x, y, 128, Some(clut), i32::from(bright));
            }
        }

        // Game-over scripts (`func_80030070` mode 2), OT layer 0 and queued before the HUD
        // numbers, so they end up in front of them.
        for o in session.stage().visible() {
            let f = o.frame.unwrap();
            if f.w <= 0 || f.h <= 0 {
                continue;
            }
            let (x, y, dw, dh, flip) = sdchr::placement(o, &f);
            out.extend_from_slice(&[
                i32::from(f.page),
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

    /// One `func_8001D29C` escape glyph; returns the pen x after it.
    fn glyph(&self, out: &mut Vec<i32>, f: &Frame, x: i32, y: i32, clut: i16) -> i32 {
        push(out, f, x, y, 128, Some(clut), NORMAL);
        x + i32::from(f.w)
    }
}
