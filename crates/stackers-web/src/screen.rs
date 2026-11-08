//! The whole 320×240 solo screen as one draw list: field, queue window, HUD panels and frame.
//!
//! The background ([`Screen::background`]) is a full-screen image the page draws first; this
//! list goes on top, back to front. The original sorts by OT layer (the frame is on layer 1,
//! everything else on layer 2, nothing here overlaps within a layer except as listed).

use crate::session::Session;
use crate::sprites::Sprites;
use stackers_formats::anm::Frame;
use stackers_formats::hud::{self, SoloScreen, Stage};
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
        Ok(Screen { layout: SoloScreen::from_exe(exe).map_err(crate::js_err)?, stage: Stage::St22 })
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
        let queue = &session.game().queue;
        for (slot, &(pivot, satellite)) in queue.iter().enumerate() {
            let y = hud::QUEUE_Y[slot];
            let scale = hud::queue_scale(y);
            let cell = scale / 8;
            let x = hud::QUEUE_X + (16 - cell) / 2;
            for (color, dy) in [(satellite, 0), (pivot, cell)] {
                if let Some(f) = sprites.still_frame(color) {
                    push(&mut out, &f, x, y + hud::QUEUE_TOP + dy, scale, None, NORMAL);
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
        out
    }
}
