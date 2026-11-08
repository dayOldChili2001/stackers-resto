//! The solo-mode screen around the field: background, window frame, next-pair window and the
//! HUD panels. Sprite records are read from the executable; positions are the constants the
//! draw code passes (see "Screen layout" in `docs/gameplay.md`).
//!
//! All screen sprites go through `func_8001B148` / `func_8001C7A8` / `func_8001B860`: a frame
//! record (see [`Frame`]) drawn at `(x + dx·s, y + dy·s)` with size `(w + (s ≠ 128))·s`, where
//! `s` is a scale out of 128. `page` indexes the uploaded-TIM slots: tpage `D_80066278[page]`,
//! CLUT `D_80103250[page][clut]`.

use crate::anm::Frame;
use crate::{exe::Exe, Error};

/// W_ASA frame and next window, drawn at (0, 0) by `func_8001A7BC` (OT layer 1, over the field).
const WINDOW: u32 = 0x8005_6820;
/// COMMON panel records `D_8005666C[i]`: 0 = monitor, 1–6 labels, 7–9 menu words, 10 label.
const PANELS: u32 = 0x8005_666C;
const PANEL_COUNT: usize = 11;
/// ST22B patch below the next window, drawn with slot 0 of the queue (`func_80023294`).
const QUEUE_PATCH: u32 = 0x8005_5F14;

/// Screen = field pixel + this (`func_800195FC`: x + 8, y − camera − 80 with camera −16).
pub const FIELD_ORIGIN: (i32, i32) = (8, -64);

/// Queue (`func_80023294`): x = `D_800B0F90` (0x93 in solo, `func_80017480`).
pub const QUEUE_X: i32 = 0x93;
/// Resting slot y (`func_80023474`); a pair is drawn at y + [`QUEUE_TOP`].
pub const QUEUE_Y: [i32; 3] = [0x28, 0x41, 0x5A];
pub const QUEUE_TOP: i32 = 0x2C;
/// Slot scale = max(128 − y, this).
pub const QUEUE_MIN_SCALE: i32 = 0x5B;
pub const QUEUE_PATCH_AT: (i32, i32) = (0x80, 0x84);
/// `func_8001B860` brightness for the patch (128 = unchanged).
pub const QUEUE_PATCH_BRIGHTNESS: i32 = 0x60;

/// HUD panels (`func_8001A560`): label then monitor at (200, y), both with the panel's palette.
pub const PANEL_X: i32 = 0xC8;
pub const PANEL_Y: [i32; 3] = [0x10, 0x58, 0xA0];
/// Arguments of the solo call in `func_8001A7BC`: palettes (1, 2, 4), labels (1, 2, 6) =
/// "Erase Number", "Record", "Course Time"; the monitors are semi-transparent.
pub const SOLO_PANEL_CLUTS: [i16; 3] = [1, 2, 4];
pub const SOLO_PANEL_LABELS: [usize; 3] = [1, 2, 6];

/// Texture pages used by the solo screen and the disc file uploaded into each (solo-mode
/// loaders `func_80017744`, `func_800183C4`, `func_80018464`). Page 18 is the stage's 8-bit
/// background copy; [`background`] gives the matching full-screen image.
pub fn page_file(page: i16, stage: Stage) -> Option<String> {
    Some(match page {
        0..=9 => format!("DERODERO/GRAPH/DERO/DERO{page}.TIM"),
        12 => "DERODERO/GRAPH/PSFONT.TIM".into(),
        13 => "DERODERO/GRAPH/SOLO/COMMON.TIM".into(),
        18 => format!("DERODERO/GRAPH/SOLO/{}B.TIM", stage.name()),
        24 => "DERODERO/GRAPH/SOLO/W_ASA.TIM".into(),
        _ => return None,
    })
}

/// The two solo stages, picked by `D_800C2FB8` in `func_80018464` (0 → ST22, else ST23).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    St22,
    St23,
}

impl Stage {
    fn name(self) -> &'static str {
        match self {
            Stage::St22 => "ST22",
            Stage::St23 => "ST23",
        }
    }
}

/// The 320×240 16-bit background, loaded to `0x80138000` and copied straight into the frame
/// buffer by `func_8001A504` every frame.
pub fn background(stage: Stage) -> String {
    format!("DERODERO/GRAPH/SOLO/{}A.TIM", stage.name())
}

#[derive(Clone, Debug)]
pub struct SoloScreen {
    pub window: Frame,
    pub panels: Vec<Frame>,
    pub queue_patch: Frame,
}

impl SoloScreen {
    pub fn from_exe(exe: &[u8]) -> Result<Self, Error> {
        let exe = Exe::parse(exe)?;
        let frame = |addr: u32| -> Result<Frame, Error> {
            Frame::from_bytes(exe.at(addr, Frame::SIZE)?).ok_or(Error::Truncated)
        };
        let window = frame(WINDOW)?;
        let panels = (0..PANEL_COUNT as u32)
            .map(|i| frame(PANELS + i * Frame::SIZE as u32))
            .collect::<Result<Vec<_>, _>>()?;
        let queue_patch = frame(QUEUE_PATCH)?;
        if window.page != 24 || panels[0].page != 13 || queue_patch.page != 18 {
            return Err(Error::Unsupported("solo screen records"));
        }
        Ok(SoloScreen { window, panels, queue_patch })
    }
}

/// Queue slot scale (out of 128) at slot y (`func_80023294`).
pub fn queue_scale(y: i32) -> i32 {
    (128 - y).max(QUEUE_MIN_SCALE)
}
