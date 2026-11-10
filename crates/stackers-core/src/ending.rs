//! The ending (mode 16, `func_80030798`), reached when a vs CPU course is cleared.
//!
//! State `D_80100410` (`jtbl_80011ED0`), timer `D_8005F3B4`:
//! - 0 (`func_80030CF0` loads the art): XA 9, then by course `D_800C30C8` and last stage
//!   `D_800C2FE0` one scene script and its object count `D_8005EC90`: last stage 14 (a 14-stage
//!   course cleared without a continue) → `0x153`, 16 objects; else course 0 → `0x150`, 5;
//!   course 1 → `0x152`, 14; course 2 → `0x154`, 14. Brightness `D_80103610[0..count]` = 0x80,
//!   `[count]` (the KABEMONO wall behind the characters) = 0x40.
//! - 1: the scene (SD draw mode 3: frame palettes, brightness by slot) for 0x259 frames.
//! - 2: fade, slots below `count` −2 and the wall −1 a frame, until slot 0 is 0.
//! - 3: course 0 ends here (mode 0, the title). Otherwise every slot below `count` is killed.
//! - 4: `func_80030F7C` loads the staff roll's art. 5: XA 8, staff page `D_8005F3B8` = `0x155`.
//! - 6: staff pages `0x155`–`0x15E`, 0x21D frames each (slot 1 at 0x40 for the last); after
//!   the last, mode 0.
//!
//! States 0, 3, 4 and 5 and the frame between staff pages draw nothing.

/// The scene and staff scripts (`SD_CHR.OBJ` ids, the low byte of `D_800C3514`).
pub mod script {
    pub const BEGINNER: u8 = 0x50;
    pub const NORMAL: u8 = 0x52;
    /// A 14-stage course cleared without a continue (the king joins in).
    pub const FULL: u8 = 0x53;
    pub const HARD: u8 = 0x54;
    pub const STAFF_FIRST: u8 = 0x55;
    pub const STAFF_LAST: u8 = 0x5E;
}

/// XA entries (`func_80040A7C`): the scene, then the staff roll.
pub const XA_SCENE: u8 = 9;
pub const XA_STAFF: u8 = 8;
/// State 1 lasts until the timer reaches this.
pub const SCENE_FRAMES: u16 = 0x259;
/// Frames each staff page is up.
pub const STAFF_FRAMES: u16 = 0x21D;
const FULL_BRIGHT: u8 = 0x80;
const WALL_BRIGHT: u8 = 0x40;
/// The 14-stage courses' last stage (`D_800C2FE0` before any continue).
const FULL_LAST: u8 = 14;

/// Which art set is loaded (texture pages).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    None,
    /// `func_80030CF0`: KABEMONO (page 20), HIRAPANE (21) and the course's characters.
    Scene { course: u8 },
    /// `func_80030F7C`: STAFF1–9 (pages 0–8) and ENDKABE1–5 (21–25).
    Staff,
}

#[derive(Clone, Debug)]
pub struct Ending {
    pub state: u8,
    /// `D_8005F3B4`.
    pub timer: u16,
    pub course: u8,
    pub last_stage: u8,
    /// The scene script (0 for a course without an ending).
    pub scene: u8,
    /// `D_8005EC90`.
    pub count: usize,
    /// `D_80103610`: SD brightness per object slot.
    pub bright: [u8; 25],
    /// `D_8005F3B8`: the staff page, `0x155`–`0x15F`.
    pub staff: u16,
    pub art: Art,
    /// This frame: a script to start (`func_8002EC34`) before the SD step.
    pub start: Option<u8>,
    /// This frame: kill objects `0..count` (state 3).
    pub kill: bool,
    /// This frame: run the SD scripts and draw them (`func_8002EC00`, `func_800194D4`).
    pub draw: bool,
    /// This frame: an XA request.
    pub xa: Option<u8>,
    /// Mode 0: back to the title.
    pub done: bool,
}

impl Ending {
    pub fn new(course: u8, last_stage: u8) -> Ending {
        Ending {
            state: 0,
            timer: 0,
            course,
            last_stage,
            scene: 0,
            count: 0,
            bright: [0; 25],
            staff: 0,
            art: Art::None,
            start: None,
            kill: false,
            draw: false,
            xa: None,
            done: false,
        }
    }

    pub fn step(&mut self) {
        self.start = None;
        self.kill = false;
        self.draw = false;
        self.xa = None;
        if self.done {
            return;
        }
        match self.state {
            0 => {
                self.art = Art::Scene { course: self.course };
                self.timer = 0;
                self.state = 1;
                self.xa = Some(XA_SCENE);
                let (scene, count) = if self.last_stage == FULL_LAST {
                    (script::FULL, 16)
                } else {
                    match self.course {
                        0 => (script::BEGINNER, 5),
                        1 => (script::NORMAL, 14),
                        2 => (script::HARD, 14),
                        _ => {
                            self.done = true;
                            return;
                        }
                    }
                };
                self.scene = scene;
                self.count = count;
                self.start = Some(scene);
                self.bright[..count].fill(FULL_BRIGHT);
                self.bright[count] = WALL_BRIGHT;
            }
            1 => {
                self.timer += 1;
                self.draw = true;
                if self.timer >= SCENE_FRAMES {
                    self.state = 2;
                }
            }
            2 => {
                for b in &mut self.bright[..self.count] {
                    *b = b.wrapping_sub(2);
                }
                self.bright[self.count] = self.bright[self.count].wrapping_sub(1);
                if self.bright[0] == 0 {
                    self.state = 3;
                }
                self.draw = true;
            }
            3 => {
                if self.course == 0 {
                    self.done = true;
                } else {
                    self.kill = true;
                    self.state = 4;
                }
            }
            4 => {
                self.art = Art::Staff;
                self.state = 5;
            }
            5 => {
                self.state = 6;
                self.staff = 0x155;
                self.timer = 0;
                self.bright[0] = FULL_BRIGHT;
                self.bright[1] = FULL_BRIGHT;
                self.xa = Some(XA_STAFF);
            }
            _ => {
                if self.staff >= 0x15F {
                    self.done = true;
                } else if self.timer == 0 {
                    self.start = Some(self.staff as u8);
                    if self.staff == 0x15E {
                        self.bright[1] = WALL_BRIGHT;
                    }
                    self.draw = true;
                    self.timer += 1;
                } else if self.timer >= STAFF_FRAMES {
                    self.staff += 1;
                    self.timer = 0;
                } else {
                    self.draw = true;
                    self.timer += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(e: &mut Ending) -> (u32, Vec<u8>, Vec<u8>) {
        let (mut frames, mut starts, mut xa) = (0, Vec::new(), Vec::new());
        while !e.done {
            e.step();
            frames += 1;
            starts.extend(e.start);
            xa.extend(e.xa);
            assert!(frames < 20_000);
        }
        (frames, starts, xa)
    }

    #[test]
    fn beginner_has_no_staff_roll() {
        let mut e = Ending::new(0, 3);
        let (frames, starts, xa) = run(&mut e);
        assert_eq!(starts, [script::BEGINNER]);
        assert_eq!(xa, [XA_SCENE]);
        assert_eq!(e.count, 5);
        // init, 0x259 scene frames, 64 fade frames, then state 3 ends it.
        assert_eq!(frames, 1 + 0x259 + 64 + 1);
        assert_eq!(e.bright[5], 0);
    }

    #[test]
    fn full_course_rolls_ten_staff_pages() {
        let mut e = Ending::new(2, 14);
        e.step();
        assert_eq!(e.start, Some(script::FULL));
        assert_eq!((e.count, e.bright[15], e.bright[16]), (16, 0x80, 0x40));
        let (frames, starts, xa) = run(&mut e);
        assert_eq!(starts, (script::STAFF_FIRST..=script::STAFF_LAST).collect::<Vec<_>>());
        assert_eq!(xa, [XA_STAFF]);
        assert_eq!(e.bright[1], 0x40);
        // (init ran above) scene 0x259 + fade 64 + states 3/4/5 + 10 × (0x21D drawn + 1 blank)
        // + the frame that ends it.
        assert_eq!(frames, 0x259 + 64 + 3 + 10 * (0x21D + 1) + 1);
    }

    #[test]
    fn continued_courses_pick_their_scene() {
        for (course, scene) in [(1, script::NORMAL), (2, script::HARD)] {
            let mut e = Ending::new(course, 13);
            e.step();
            assert_eq!((e.start, e.count), (Some(scene), 14));
        }
    }
}
