//! The whole 320×240 solo screen as one draw list: field, queue window, HUD panels and frame.
//!
//! The background ([`Screen::background`]) is a full-screen image the page draws first; this
//! list goes on top, back to front. The original sorts by OT layer (the frame is on layer 1,
//! everything else on layer 2, nothing here overlaps within a layer except as listed).

use crate::session::Session;
use crate::sprites::Sprites;
use stackers_formats::anm::Frame;
use stackers_formats::hud::{self, SoloScreen, Stage};
use stackers_formats::sdchr;
use wasm_bindgen::prelude::*;

/// Ints per sprite in [`Screen::solo`]:
/// `[page, clut, u, v, w, h, x, y, draw_w, draw_h, flip, mode]`, `mode` = brightness (128 =
/// unchanged) | `semi << 8` (STP texels blend 50/50).
pub const SCREEN_STRIDE: usize = 12;
const NORMAL: i32 = 128;
const SEMI: i32 = 1 << 8;

#[wasm_bindgen]
pub struct Screen {
    layout: SoloScreen,
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
        Ok(Screen { layout: SoloScreen::from_exe(exe).map_err(crate::js_err)?, stage: Stage::St22, record: 0 })
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

    /// Disc path of the TIM uploaded into texture page `page`, if the solo screen uses it.
    pub fn page_file(&self, page: i32) -> Option<String> {
        hud::page_file(i16::try_from(page).ok()?, self.stage)
    }

    /// Every texture page [`Screen::solo`] can reference.
    pub fn pages(&self) -> Vec<i32> {
        (0..32).filter(|&p| self.page_file(p).is_some()).collect()
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
        out
    }
}

impl Screen {
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
