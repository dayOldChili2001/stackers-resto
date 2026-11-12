//! The ending (mode 16) exposed to JavaScript: see `stackers_core::ending`. Drawn by
//! `Screen::ending`.

use stackers_core::ending::{self, Art};
use stackers_formats::sdchr::{SdBank, SdStage};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Ending {
    e: ending::Ending,
    sd: Option<SdBank>,
    stage: SdStage,
    sounds: Vec<u8>,
}

impl Ending {
    pub(crate) fn inner(&self) -> &ending::Ending {
        &self.e
    }

    pub(crate) fn stage(&self) -> &SdStage {
        &self.stage
    }
}

#[wasm_bindgen]
impl Ending {
    /// After clearing vs CPU course `course` (0–2) whose last stage was `last_stage`
    /// (`D_800C2FE0`: 14 unless a continue cut the course short).
    #[wasm_bindgen(constructor)]
    pub fn new(course: u8, last_stage: u8) -> Ending {
        Ending { e: ending::Ending::new(course, last_stage), sd: None, stage: SdStage::default(), sounds: Vec::new() }
    }

    /// `DATA/SD_CHR.OBJ`: the scene and staff scripts.
    pub fn set_sd_chr(&mut self, data: &[u8]) -> Result<(), JsError> {
        self.sd = Some(SdBank::parse(data).map_err(crate::js_err)?);
        Ok(())
    }

    /// Advance one frame (the ending takes no input).
    pub fn step(&mut self) {
        self.e.step();
        self.sounds.clear();
        let Some(bank) = &self.sd else { return };
        if self.e.kill {
            for o in &mut self.stage.objects[..self.e.count] {
                o.layer = -1;
            }
        }
        if let Some(id) = self.e.start {
            self.stage.start(bank, id);
        }
        if self.e.draw && self.stage.active {
            self.sounds = self.stage.step(bank);
        }
    }

    pub fn sounds(&self) -> Vec<u8> {
        self.sounds.clone()
    }

    /// True once the ending is over (mode 0: the title).
    #[wasm_bindgen(getter)]
    pub fn done(&self) -> bool {
        self.e.done
    }

    /// An XA request this frame (`func_80040A7C`: 9 the scene, 8 the staff roll), else −1.
    #[wasm_bindgen(getter)]
    pub fn xa(&self) -> i32 {
        self.e.xa.map_or(-1, i32::from)
    }

    /// `D_80100410`: 1 the scene, 2 its fade, 6 the staff roll (0, 3–5 load, nothing drawn).
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> u32 {
        u32::from(self.e.state)
    }

    /// The staff page on screen (1–10), 0 before the staff roll.
    #[wasm_bindgen(getter)]
    pub fn staff_page(&self) -> u32 {
        match self.e.art {
            Art::Staff if self.e.staff >= 0x155 => u32::from(self.e.staff - 0x154).min(10),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    const TRACK01: &str = "../../rom/Tecmo Stackers (USA)/Tecmo Stackers (USA) (Track 01).bin";

    /// One frame: the ending's state, timer, staff page, brightness, XA request and SEs, then
    /// every live SD object (slot, layer, flip, x, y, scale, frame record).
    fn line(e: &Ending) -> String {
        let i = &e.e;
        let mut s = format!("{} {} {} {} |", i.state, i.timer, i.staff, i.done as u8);
        for b in i.bright {
            write!(s, " {b}").unwrap();
        }
        write!(s, " | {} [{}] |", i.xa.map_or(-1, i32::from), e.sounds.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")).unwrap();
        for (n, o) in e.stage.objects.iter().enumerate().filter(|(_, o)| o.alive()) {
            let f = o.frame.unwrap_or_default();
            write!(
                s,
                " {n}:{},{},{},{},{},{}/{},{},{},{},{},{},{},{},{}",
                o.layer, o.flip, o.x, o.y, o.scale[0], o.scale[1], f.u, f.v, f.w, f.h, f.dx, f.dy, f.page, f.clut, f.flip
            )
            .unwrap();
        }
        s
    }

    /// `FRAME_DUMP=/tmp/n.txt cargo test -p stackers-web ending::tests::frame_dump -- --ignored`
    /// then `python3 tools/framediff.py ending /tmp/n.txt`: every course / last stage pair, from
    /// mode 16's first frame until it hands back to the title.
    #[test]
    #[ignore]
    fn frame_dump() {
        use stackers_formats::iso::Disc;
        let path = std::env::var("FRAME_DUMP").expect("FRAME_DUMP");
        let raw = std::fs::read(TRACK01).expect("disc");
        let disc = Disc::open(&raw).unwrap();
        let sd = disc.read_path("DERODERO/DATA/SD_CHR.OBJ").unwrap();
        let mut out = String::new();
        for (course, last) in [(0, 3), (0, 14), (1, 13), (2, 13), (1, 14), (2, 14), (3, 13)] {
            let mut e = Ending::new(course, last);
            e.set_sd_chr(&sd).unwrap();
            writeln!(out, "N {course} {last}").unwrap();
            let mut frames = 0;
            while !e.e.done && frames < 20_000 {
                e.step();
                frames += 1;
                writeln!(out, "N {}", line(&e)).unwrap();
            }
        }
        std::fs::write(path, out).unwrap();
    }
}
