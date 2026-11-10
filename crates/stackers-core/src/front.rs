//! The front end: boot, title and mode menu, up to the start of a solo (mode 4) game.
//!
//! Modes (`D_80100684`, dispatcher `func_80014634`): 0 a 4-frame blank (`func_80015850`),
//! 1 the title sequence (`func_800158FC` → `func_80027240` init, `func_80027300` per frame),
//! 8 the mode menu (`func_80015960` → `func_80027950`, `func_80027A64`), 9 the hand-off into a
//! game (`func_80015A24` → `func_8002B294`, `func_8002B380`, `func_8002B454`). The menu's
//! "Insane Mode" is the solo screen, mode 4; Chain Reaction is mode 7; Time Trial goes through
//! the course list (mode 12, `func_80016B5C`) to mode 11; Arcade is mode 5 ([`arcade`]), whose
//! two-player branch ends in mode 14. Option isn't ported, so picking it plays the menu's exit
//! and comes back. Mode 2 (the attract demo the title falls into after
//! 40 idle seconds) isn't ported either: the title restarts instead.
//!
//! Everything here is state; drawing and the SD_CHR script VM live in `stackers-web`, which
//! runs [`Front::sd`] once per frame the way `func_8002EAD4`/`func_8002EB08` do.

use crate::game::pad;

pub mod arcade;

pub mod mode {
    pub const BLANK: u8 = 0;
    pub const TITLE: u8 = 1;
    pub const SOLO: u8 = 4;
    pub const ARCADE: u8 = 5;
    pub const CHAIN: u8 = 7;
    pub const MENU: u8 = 8;
    pub const LAUNCH: u8 = 9;
    pub const TIME_TRIAL: u8 = 11;
    pub const COURSE: u8 = 12;
    /// Player 1 vs CPU (`func_80016480`).
    pub const VS_CPU: u8 = 13;
    /// Player 1 vs Player 2, a set match (`func_80015FCC`).
    pub const VERSUS: u8 = 14;
}

/// Menu items in cursor order (`D_800B1400`) and the mode each one picks (`D_801026DA`).
pub mod item {
    pub const ARCADE: u8 = 0;
    pub const INSANE: u8 = 1;
    pub const TIME_TRIAL: u8 = 2;
    pub const CHAIN: u8 = 3;
    pub const OPTION: u8 = 4;
    pub const COUNT: i8 = 5;
    /// `func_80027A64`: the mode number stored for each item.
    pub const MODE: [u8; 5] = [6, 4, 11, 7, 17];
    /// Sound effect when the item is confirmed (after 0xA8).
    pub const SE: [u8; 5] = [0xA1, 0xA2, 0xA3, 0xA4, 0x9F];
    /// SD_CHR slot of the item's label (its palette becomes 1 when confirmed).
    pub const SLOT: [usize; 5] = [1, 2, 3, 4, 6];
}

/// SD_CHR object slots whose palette the front end sets (`D_80103658`, i16 × 25).
pub const SLOTS: usize = 25;

/// One run of the SD_CHR VM this frame: start `start` first (a `D_800C3514` request), then
/// step and draw every object with draw mode `mode` (`D_8005EC8C`: 0 full brightness,
/// 1 [`Front::brightness`]); each object's CLUT is [`Front::palettes`]`[slot]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SdRun {
    pub start: Option<u8>,
    pub mode: u8,
}

/// CD-DA requests (`func_80040AE8(index, 1)` / `func_80040A7C(1, 2)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Music {
    Play(u8),
    Stop,
}

/// What `func_80027300` draws itself in title states 0 and 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Splash {
    /// TECMO logo (`D_80056E84`), `func_8001C03C` centred on (160, 100), vertical scale /128.
    Tecmo { scale_y: i32 },
    /// The title logo (`D_80055C78`) as a GsSPRITE (`func_8001CF84`) centred on the screen:
    /// scale /4096, rotation in 1/4096 degrees.
    Spin { scale: i32, rotate: i32 },
}

/// The menu's arrow (`D_80055C8C[frame]` by `func_8001B860`, layer 3) and the item's
/// description (`D_80056C70[item]` at (0x20, 0x50) by `func_8001B148`, layer 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuDraw {
    pub arrow_frame: usize,
    pub arrow: (i32, i32),
    pub description: Option<usize>,
}

/// An arrow `D_80055C8C[frame]` drawn by `func_8001B860` on layer 3 (under the SD objects);
/// `flip` mirrors it (player 2's arrow on the Arcade difficulty list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrow {
    pub frame: usize,
    pub at: (i32, i32),
    pub flip: bool,
}

/// Sprites the Arcade screens draw on layer 0 before the SD VM runs: in front of the scripts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    /// The set-match count `D_80056A18[choice]` (LEVEL page: "1", "3", "5").
    Sets { choice: usize, at: (i32, i32) },
    /// `func_8001D778`'s half for one player: the □ label (`D_80056CE0`, MSG2) at
    /// (x, [`arcade::STOCK_Y`]) and the stock as one 8×8 digit (font 2, palette 3) at
    /// player 1: (x + 0x10, y + 8), player 2: (x − 8, y + 8).
    Stock { player: usize, value: u8, x: i32 },
}

pub const DESCRIPTION_AT: (i32, i32) = (0x20, 0x50);
/// `D_80100368`: arrow y for each item.
pub const ARROW_Y: [i32; 5] = [0x43, 0x63, 0x83, 0xA3, 0xC3];
pub const ARROW_X: i32 = 0x82;
/// Mode 12's arrow x (`D_800B1344` = 0x28) and course count.
pub const COURSE_ARROW_X: i32 = 0x28;
const COURSES: i8 = 5;
/// Idle arrow frame; after a confirm it plays frames 6–31, then the menu leaves.
pub(crate) const ARROW_REST: usize = 5;
pub(crate) const ARROW_END: usize = 0x20;

#[derive(Clone, Debug)]
pub struct Front {
    /// `D_80100684`.
    pub mode: u8,
    /// `D_800C34AA`: per-mode sub-state.
    sub: u8,
    /// `D_800C2EC0`: state inside the sub-state.
    state: u8,
    /// `D_801003DC`.
    t: u32,
    /// `D_801003D8`: how long the menu's exit lasts.
    exit_len: u32,
    /// `D_800C3514`: SD_CHR script to start at the next VM run (`func_8002EC34`).
    pending: Option<u8>,
    prev: [u16; 2],
    /// `D_801036A4`: brightness of SD draw mode 1 (the title fades it out before the demo).
    pub brightness: u8,
    /// `D_80103658`: CLUT of each SD object in draw modes 0 and 1.
    pub palettes: [i16; SLOTS],
    /// `D_80066AD4` and `D_8010ADF8`: the spinning logo's scale and angle.
    spin_scale: u16,
    spin_angle: i32,
    /// GsBG scroll (`D_800696A0 + 0x38`): +1, −1 per frame while the background is drawn.
    pub bg_scroll: (i16, i16),
    /// `D_800B1400`, `D_800B1340` (0 choosing, 1 confirmed, 2 arrow plays, 3 leave),
    /// `D_800B1338`, `D_800B1344`/`D_800B1348`.
    pub cursor: i8,
    menu_state: u8,
    arrow_frame: usize,
    arrow: (i32, i32),
    /// `D_801026DA`: the mode the menu picked.
    pub choice: u8,
    /// `D_800699D4`: the Time Trial course picked in mode 12.
    pub course: u8,
    /// `D_80109638`: L1 + R1 + ✕ held when confirming Insane, Chain Reaction or Option.
    /// What it unlocks isn't traced.
    pub secret: bool,
    /// `D_800C2FE6`: the set-match choice (0–2) and `D_80103692`, its set count.
    pub set_match: i8,
    pub sets: u8,
    /// `D_800B0050`: which player the CPU controls (`arcade::NO_CPU` in mode 14).
    pub cpu: u8,
    /// `D_80102740[p]`: difficulty 1–5 (Easy … Insanity), the row of `D_800120D4`.
    pub difficulty: [u8; 2],
    /// `D_800FFCF8[p]`: □ uses per set.
    pub stock: [u8; 2],
    /// `D_800662F0`: the level row the Arcade screens reset (the first stage of a vs-CPU course).
    pub level_row: u8,
    /// `D_800C30C8`: the vs-CPU course's level table (0 Practice, 1 Normal, 2 Hard).
    pub vs_table: u8,
    /// `D_800C2FE0`: the stage that ends the vs-CPU course.
    pub last_stage: u8,
    picks: [arcade::Pick; 2],

    // Per-frame output.
    pub sd: Option<SdRun>,
    pub sounds: Vec<u8>,
    pub music: Option<Music>,
    pub splash: Option<Splash>,
    pub menu: Option<MenuDraw>,
    pub arrows: Vec<Arrow>,
    pub overlays: Vec<Overlay>,
    /// The tiled background (`func_800418CC`) was drawn this frame.
    pub bg: bool,
}

impl Default for Front {
    fn default() -> Self {
        Front::new()
    }
}

impl Front {
    /// Power-on: mode 0 (`func_80014634` clears the mode and its counters).
    pub fn new() -> Front {
        Front {
            mode: mode::BLANK,
            sub: 0,
            state: 0,
            t: 0,
            exit_len: 0,
            pending: None,
            prev: [0; 2],
            brightness: 0x80,
            palettes: [0; SLOTS],
            spin_scale: 0,
            spin_angle: 0,
            bg_scroll: (0, 0),
            cursor: 0,
            menu_state: 0,
            arrow_frame: ARROW_REST,
            arrow: (ARROW_X, ARROW_Y[0]),
            choice: 0,
            course: 0,
            secret: false,
            set_match: 0,
            sets: arcade::SETS[0],
            cpu: arcade::NO_CPU,
            difficulty: [1; 2],
            stock: [0; 2],
            level_row: 0,
            vs_table: 0,
            last_stage: 0,
            picks: [arcade::Pick::default(); 2],
            sd: None,
            sounds: Vec::new(),
            music: None,
            splash: None,
            menu: None,
            arrows: Vec::new(),
            overlays: Vec::new(),
            bg: false,
        }
    }

    /// Back to the title after a game (Quit, or the game-over countdown ran out): mode 0.
    pub fn to_title() -> Front {
        Front::new()
    }

    /// The front end is done and a solo game should start.
    pub fn launch_solo(&self) -> bool {
        self.mode == mode::SOLO
    }

    /// The front end is done and a Chain Reaction game should start.
    pub fn launch_chain(&self) -> bool {
        self.mode == mode::CHAIN
    }

    /// The front end is done and a Time Trial (mode 11) on [`Front::course`] should start.
    pub fn launch_trial(&self) -> bool {
        self.mode == mode::TIME_TRIAL
    }

    /// The front end is done and a two-player set match (mode 14) should start, with
    /// [`Front::difficulty`], [`Front::stock`] and [`Front::sets`].
    pub fn launch_versus(&self) -> bool {
        self.mode == mode::VERSUS
    }

    /// The front end is done and a vs-CPU course (mode 13) should start: [`Front::vs_table`],
    /// first stage [`Front::level_row`], ending at [`Front::last_stage`]. The CPU is
    /// player 2 (the menu's pad is pad 1).
    pub fn launch_vs_cpu(&self) -> bool {
        self.mode == mode::VS_CPU
    }

    /// One 60 Hz frame with pad 1 only. `held` uses the [`pad`] bits.
    pub fn step(&mut self, held: u16) {
        self.step2([held, 0]);
    }

    /// One 60 Hz frame with both pads. Only the Arcade difficulty list reads pad 2.
    pub fn step2(&mut self, held2: [u16; 2]) {
        let pressed2 = [held2[0] & !self.prev[0], held2[1] & !self.prev[1]];
        self.prev = held2;
        let (held, pressed) = (held2[0], pressed2[0]);
        self.sd = None;
        self.sounds.clear();
        self.music = None;
        self.splash = None;
        self.menu = None;
        self.arrows.clear();
        self.overlays.clear();
        self.bg = false;
        match self.mode {
            mode::BLANK => {
                // func_80015850: four blank frames, then the title.
                self.t += 1;
                if self.t >= 4 {
                    self.set_mode(mode::TITLE);
                }
            }
            mode::TITLE => match self.sub {
                0 => self.title_init(),
                _ => self.title(held, pressed),
            },
            mode::MENU => match self.sub {
                0 => self.menu_init(),
                _ => self.menu_step(held, pressed),
            },
            // func_8003A7DC (clears the scores) runs first; nothing here keeps them.
            mode::ARCADE => self.arcade(held2, pressed2),
            mode::COURSE => match self.sub {
                0 => self.course_init(),
                _ => self.course_step(pressed),
            },
            mode::LAUNCH => match self.sub {
                0 => self.launch_init(),
                3 => self.launch_wait(),
                _ => self.launch(pressed),
            },
            _ => {}
        }
    }

    fn set_mode(&mut self, m: u8) {
        self.mode = m;
        self.sub = 0;
        self.t = 0;
    }

    /// Run the VM this frame, starting the pending request first.
    fn run_sd(&mut self, mode: u8) {
        self.sd = Some(SdRun { start: self.pending.take(), mode });
    }

    /// `func_80024640(bg, 1, −1)` + `func_800418CC`.
    fn draw_bg(&mut self) {
        self.bg_scroll.0 = self.bg_scroll.0.wrapping_add(1);
        self.bg_scroll.1 = self.bg_scroll.1.wrapping_sub(1);
        self.bg = true;
    }

    /// `func_8002EAD4`: SD draw mode 1 + the background, the common tail of most states.
    fn scripts_and_bg(&mut self) {
        self.run_sd(1);
        self.draw_bg();
    }

    /// Palettes the title sets before the menu comes in (`func_80027300` state 3, and the
    /// menu's "back" path).
    fn menu_palettes(&mut self) {
        self.palettes[0] = 2;
        for s in [1, 2, 3, 4, 6] {
            self.palettes[s] = 0;
        }
        self.palettes[5] = 7;
        for s in [7, 8, 9] {
            self.palettes[s] = 2;
        }
    }

    /// `func_80027240`.
    fn title_init(&mut self) {
        self.sub = 1;
        self.brightness = 0x80;
        self.state = 0;
        self.t = 0;
        self.exit_len = 0;
        self.spin_angle = 0;
        self.spin_scale = 0;
        self.palettes = [0; SLOTS];
        self.palettes[0] = 2;
        self.palettes[5] = 7;
        self.palettes[7] = 2;
        self.palettes[8] = 2;
        // func_8002430C(0): the background map starts unscrolled.
        self.bg_scroll = (0, 0);
        self.secret = false;
    }

    /// Start pressed on this frame (`D_800B193C == 1`: the per-button hold counter of
    /// `func_80014F20`; pad 2's counter `D_800B197C` isn't ported).
    fn start(pressed: u16) -> bool {
        pressed & pad::START != 0
    }

    /// `func_80027300`.
    fn title(&mut self, _held: u16, pressed: u16) {
        self.t += 1;
        let t = self.t;
        match self.state {
            // TECMO logo: stretches open over 64 frames, holds, closes, then the title music.
            0 => {
                if Self::start(pressed) {
                    self.state = 2;
                    self.t = 0;
                    self.music = Some(Music::Play(0));
                    self.request_title(0x03);
                } else if t < 0x40 {
                    self.splash = Some(Splash::Tecmo { scale_y: (t as i32) * 2 });
                } else if t - 0x40 < 0xAC {
                    self.splash = Some(Splash::Tecmo { scale_y: 0x80 });
                } else if t - 0xEC < 0x40 {
                    self.splash = Some(Splash::Tecmo { scale_y: 0x80 - 2 * (t as i32 - 0xEC) });
                } else if t == 0x12C {
                    self.brightness = 0x80;
                    self.t = 0;
                    self.state = 1;
                    self.music = Some(Music::Play(0));
                }
            }
            // The logo spins in, growing; at 0x78 the title script takes over.
            1 => {
                if Self::start(pressed) {
                    self.state = 2;
                    self.t = 0;
                    self.request_title(0x03);
                } else if t < 0x78 {
                    self.spin_angle += 0x1E000;
                    if self.spin_angle > 0x167FFF {
                        self.spin_angle -= 0x168000;
                    }
                    self.spin_scale = self.spin_scale.wrapping_add(t as u16);
                    self.splash = Some(Splash::Spin {
                        scale: i32::from(self.spin_scale as i16),
                        rotate: self.spin_angle,
                    });
                } else {
                    if t == 0x78 {
                        self.palettes[0] = 0;
                        self.palettes[1] = 0;
                        self.pending = Some(0x02);
                    }
                    if t - 0x78 < 0x3C {
                        // func_8002EB08: draw mode 0, the request starts now.
                        self.state = 2;
                        self.t = 0;
                        self.run_sd(0);
                    }
                }
            }
            // Title: waits for Start; after 40 s the original runs the attract demo.
            2 => {
                self.scripts_and_bg();
                if Self::start(pressed) {
                    self.brightness = 0x80;
                    self.state = 3;
                    self.t = 0;
                    self.pending = Some(0x04);
                    self.palettes[0] = 0;
                    self.palettes[1] = 0;
                } else if t >= 0x960 {
                    self.brightness = 0x80;
                    self.state = 4;
                    self.t = 0;
                }
            }
            // The title slides away; after 20 frames the menu comes in.
            3 => {
                self.scripts_and_bg();
                if t >= 0x14 {
                    self.pending = Some(0x05);
                    self.t = 0;
                    self.state = 0;
                    self.menu_palettes();
                    self.set_mode(mode::MENU);
                }
            }
            // Fade out (SD objects only; the background isn't drawn).
            4 => {
                self.brightness = self.brightness.wrapping_add(0xFE);
                if self.brightness < 2 {
                    self.music = Some(Music::Stop);
                    self.state = 5;
                    self.t = 0;
                    self.pending = None;
                }
                self.run_sd(1);
            }
            // Blank, then mode 2 (attract demo, not ported: the title restarts).
            _ => {
                if t >= 4 {
                    self.set_mode(mode::TITLE);
                }
            }
        }
    }

    /// State 0/1 Start: jump straight to the title script (0x103), title music already set.
    fn request_title(&mut self, id: u8) {
        self.pending = Some(id);
        self.palettes[0] = 0;
        self.brightness = 0x80;
    }

    /// `func_80027950`.
    fn menu_init(&mut self) {
        self.sub = 1;
        self.brightness = 0x80;
        self.arrow = (ARROW_X, ARROW_Y[0]);
        self.state = 0;
        self.t = 0;
        self.exit_len = 0;
        self.cursor = 0;
        self.menu_state = 0;
        self.arrow_frame = ARROW_REST;
        self.scripts_and_bg();
    }

    /// `func_80027A64`.
    fn menu_step(&mut self, held: u16, pressed: u16) {
        self.t += 1;
        match self.state {
            0 => {
                self.t = 0;
                // Both pads are read (pad 2 first); only pad 1 is ported. Buttons compare the
                // whole pressed word, so two new presses on one frame do nothing.
                if self.menu_state == 0 {
                    if pressed == pad::UP {
                        self.cursor -= 1;
                        self.sounds.push(0xA7);
                    }
                    if pressed == pad::DOWN {
                        self.cursor += 1;
                        self.sounds.push(0xA7);
                    }
                    if self.cursor >= item::COUNT {
                        self.cursor = 0;
                    }
                    if self.cursor < 0 {
                        self.cursor = item::COUNT - 1;
                    }
                    if pressed == pad::START || pressed == pad::CROSS {
                        self.menu_state = 1;
                    }
                }
                let i = self.cursor as usize;
                if self.menu_state == 1 {
                    self.menu_state = 2;
                    self.sounds.push(0xA8);
                    self.choice = item::MODE[i];
                    self.palettes[item::SLOT[i]] = 1;
                    self.exit_len = 0x3C;
                    if matches!(i as u8, item::INSANE | item::CHAIN | item::OPTION) && held == 0x4C {
                        self.secret = true;
                    }
                    self.sounds.push(item::SE[i]);
                }
                if self.menu_state == 2 {
                    self.arrow_frame += 1;
                    if self.arrow_frame >= ARROW_END {
                        self.arrow_frame = ARROW_REST;
                        self.menu_state = 3;
                    }
                }
                if self.menu_state == 3 {
                    self.state = 1;
                    self.t = 0;
                    self.pending = Some(0x06);
                }
                self.arrow.1 = ARROW_Y[i];
                self.menu = Some(MenuDraw { arrow_frame: self.arrow_frame, arrow: self.arrow, description: Some(i) });
                self.scripts_and_bg();
            }
            // Leaving: the arrow flies off up-left after 20 frames; △ cancels.
            1 => {
                if self.t >= 0x14 {
                    self.arrow.0 -= 16;
                    self.arrow.1 -= 2;
                }
                self.menu = Some(MenuDraw { arrow_frame: self.arrow_frame, arrow: self.arrow, description: None });
                self.scripts_and_bg();
                if pressed == pad::TRIANGLE {
                    self.t = 0;
                    self.state = 2;
                }
                if self.t >= self.exit_len {
                    self.brightness = 0x80;
                    self.state = 0;
                    self.t = 0;
                    if matches!(self.choice, mode::SOLO | mode::CHAIN) {
                        // Mode 9, then mode 4 or 7.
                        self.music = Some(Music::Stop);
                        self.set_mode(mode::LAUNCH);
                    } else if self.choice == mode::TIME_TRIAL {
                        // Mode 12, the course select.
                        self.pending = Some(0x0B);
                        self.course_palettes([7, 8, 9]);
                        self.set_mode(mode::COURSE);
                    } else if self.choice == 6 {
                        // Arcade: mode 5.
                        self.to_arcade();
                    } else {
                        // Option (17) isn't ported: come back like △ does.
                        self.back_to_menu();
                    }
                }
            }
            // △: the arrow keeps flying, then the menu comes back in.
            _ => {
                self.arrow.0 -= 16;
                self.arrow.1 -= 2;
                self.menu = Some(MenuDraw { arrow_frame: self.arrow_frame, arrow: self.arrow, description: None });
                self.scripts_and_bg();
                if self.t >= 0x14 {
                    self.back_to_menu();
                }
            }
        }
    }

    fn back_to_menu(&mut self) {
        self.pending = Some(0x05);
        self.menu_palettes();
        self.set_mode(mode::MENU);
    }

    /// Palettes for the course list (`func_80027A64` → mode 12, and △ in mode 12): slot 0 = 3,
    /// slot 6 = 7, `twos` = 2, slots 1–5 = 0. The menu and mode-12 state 0 use slots 7–9; mode-12
    /// state 1's △ writes 8–10 instead.
    fn course_palettes(&mut self, twos: [usize; 3]) {
        self.palettes[0] = 3;
        self.palettes[6] = 7;
        for s in 1..=5 {
            self.palettes[s] = 0;
        }
        for s in twos {
            self.palettes[s] = 2;
        }
    }

    /// `func_80029B5C`: the course list, cursor on course 2.
    fn course_init(&mut self) {
        self.sub = 1;
        self.state = 0;
        self.t = 0;
        self.spin_scale = 0;
        self.cursor = 2;
        self.arrow = (COURSE_ARROW_X, ARROW_Y[2]);
        self.menu_state = 0;
        self.arrow_frame = ARROW_REST;
        self.scripts_and_bg();
    }

    /// `func_80029CA8`.
    fn course_step(&mut self, pressed: u16) {
        self.t += 1;
        match self.state {
            0 => {
                if self.t >= 0x14 {
                    self.t = 0x14;
                    if pressed == pad::TRIANGLE {
                        // Back to the mode menu.
                        self.state = 2;
                        self.brightness = 0x80;
                        self.pending = Some(0x0D);
                        self.t = 0;
                        self.course_palettes([7, 8, 9]);
                    }
                }
                if self.menu_state == 0 {
                    if pressed == pad::UP {
                        self.cursor -= 1;
                        self.sounds.push(0xA7);
                    }
                    if pressed == pad::DOWN {
                        self.cursor += 1;
                        self.sounds.push(0xA7);
                    }
                    if self.cursor >= COURSES {
                        self.cursor = 0;
                    }
                    if self.cursor < 0 {
                        self.cursor = COURSES - 1;
                    }
                    if pressed == pad::START || pressed == pad::CROSS {
                        self.menu_state = 1;
                    }
                }
                let i = self.cursor as usize;
                if self.menu_state == 1 {
                    self.menu_state = 2;
                    self.sounds.push(0xA8);
                    self.palettes[i + 1] = 1;
                    self.course = i as u8;
                }
                if self.menu_state == 2 {
                    self.arrow_frame += 1;
                    if self.arrow_frame >= ARROW_END {
                        self.arrow_frame = ARROW_REST;
                        self.menu_state = 3;
                    }
                }
                if self.menu_state == 3 {
                    self.state = 1;
                    self.t = 0;
                    self.pending = Some(0x0C);
                }
                self.arrow.1 = ARROW_Y[i];
                self.menu = Some(MenuDraw { arrow_frame: self.arrow_frame, arrow: self.arrow, description: None });
                self.scripts_and_bg();
            }
            // The list leaves; after 0x50 frames mode 9 hands over to mode 11. △ goes back.
            1 => {
                if self.t >= 0x14 {
                    self.arrow.0 -= 16;
                    self.arrow.1 -= 1;
                }
                self.menu = Some(MenuDraw { arrow_frame: self.arrow_frame, arrow: self.arrow, description: None });
                self.scripts_and_bg();
                if pressed == pad::TRIANGLE {
                    self.brightness = 0x80;
                    self.pending = Some(0x0B);
                    self.state = 0;
                    self.course_palettes([8, 9, 10]);
                    self.set_mode(mode::COURSE);
                } else if self.t >= 0x50 {
                    self.brightness = 0x80;
                    self.state = 0;
                    self.exit_len = 0;
                    self.choice = mode::TIME_TRIAL;
                    self.music = Some(Music::Stop);
                    self.set_mode(mode::LAUNCH);
                }
            }
            // △: the arrow flies off and the mode menu comes back.
            _ => {
                self.arrow.0 -= 16;
                self.arrow.1 -= 1;
                self.menu = Some(MenuDraw { arrow_frame: self.arrow_frame, arrow: self.arrow, description: None });
                self.scripts_and_bg();
                if self.t >= 0x14 {
                    self.back_to_menu();
                }
            }
        }
    }

    /// `func_8002B294`: the menu's exit script keeps playing for 60 frames.
    fn launch_init(&mut self) {
        self.sub = 3;
        self.state = 1;
        self.t = 0;
        self.exit_len = 0;
        self.scripts_and_bg();
    }

    /// `func_8002B380`. At 0x40 the original also plays the mode's voice (kind 3, id from
    /// `func_8002C708`: not ported).
    fn launch_wait(&mut self) {
        self.t += 1;
        if self.t < 0x3D {
            self.scripts_and_bg();
        }
        if self.t == 0x40 {
            self.t = 0;
            self.sub = 1;
        }
    }

    /// `func_8002B454`, entered in state 1 from the menu: two frames, the stage loads, four
    /// more frames, then the picked mode. (State 0, a ten-picture sequence on pages 0–2 with
    /// a tick every 10 frames, isn't reached on this path.)
    fn launch(&mut self, pressed: u16) {
        self.t += 1;
        if pressed == pad::START || pressed == pad::CROSS {
            self.t = 0;
            self.state = 1;
        }
        match self.state {
            1 => {
                if self.t >= 2 {
                    self.state = 2;
                }
            }
            2 => {
                self.state = 3;
                self.t = 0;
            }
            _ => {
                if self.t >= 4 {
                    self.set_mode(self.choice);
                }
            }
        }
    }
}
