//! Mode 17, the Option screen (`func_8003DDD8`), entered from the mode menu with script 0x15F.
//!
//! Three pages (`D_800C2EC0`), each with phases `D_800C2ECA` (0 coming in, 1 live, 2 leaving):
//!
//! - 0, the list: "Sound" or "Controller" (cursor `D_800B1400`, both pads), △ back to the menu.
//! - 1, Sound (`D_80100388`, `D_80103690`): the effect volume (every key-on's left/right volume,
//!   `func_800407C4`) and the CD volume (`func_8004D130(0, l, r)`, SPU CD input: CD-DA and XA),
//!   0–0x7F each, both 0x40 at power-on (`func_80040394`). ←/→ step by one with auto-repeat
//!   (`func_8003EBF4`) and draw as two blue bars over pale half-transparent ones
//!   (`func_8003EE4C`, `func_8003ED18`).
//! - 2, Controller (`D_80100374[pad]`): ←/→ on either pad picks one of three button layouts
//!   (`D_80057028`, applied by `func_80015548` in the game modes; see [`remap`]). Each pad's
//!   layout number slides in from the top (`func_8003F140`, `func_8003F044`) and its eight
//!   button labels move to the layout's rows (`func_8003F400`, `func_8003F2DC`).
//!
//! The settings live in [`Settings`], which outlives the front end (the original keeps them in
//! RAM; `Front::to_title` takes them back in).

use super::{mode, Front, MenuDraw, ARROW_END, ARROW_REST};
use crate::game::pad;

/// SD_CHR scripts of the Option screen (requested as `0x100 | id`).
pub mod script {
    /// The list comes in (from the menu, and back from either page).
    pub const LIST_IN: u8 = 0x5F;
    /// The list leaves (a page was picked, or △ back to the menu).
    pub const LIST_OUT: u8 = 0x60;
    pub const SOUND_IN: u8 = 0x61;
    pub const SOUND_OUT: u8 = 0x62;
    pub const PADS_IN: u8 = 0x63;
    pub const PADS_OUT: u8 = 0x64;
}

/// Highest volume (`D_80100388`, `D_80103690`).
pub const MAX_VOLUME: i16 = 0x7F;
/// Power-on volumes (`func_80040394`).
pub const DEFAULT_VOLUME: i16 = 0x40;
/// Button layouts (`D_80100374`).
pub const LAYOUTS: u8 = 3;

/// `D_800100B8` / `D_80057028[layout]`: for each button bit from 0x8000 down, the bit it becomes.
/// Start, Select and the 0x0600 bits map to nothing; the game keeps them through other words.
const REMAP: [[u16; 16]; 3] = [
    [0x8000, 0x4000, 0x2000, 0x1000, 0, 0, 0, 0, 0x80, 0x40, 0x20, 0x10, 8, 4, 2, 1],
    [0x8000, 0x4000, 0x2000, 0x1000, 0, 0, 0, 0, 0x40, 0x80, 0x20, 0x10, 8, 4, 2, 1],
    [0x8000, 0x4000, 0x2000, 0x1000, 0, 0, 0, 0, 0x40, 0x80, 0x10, 0x20, 8, 4, 2, 1],
];

/// A pad's buttons under button layout `layout` (`func_80015548`): layout 1 swaps □ and ✕,
/// layout 2 also swaps ○ and △. The original drops Start/Select from the remapped word (pause and
/// the menus read the raw pad); here they pass through unchanged so callers can use one word.
pub fn remap(buttons: u16, layout: u8) -> u16 {
    let table = &REMAP[usize::from(layout.min(LAYOUTS - 1))];
    let face = buttons & 0xF0FF;
    let mut out = buttons & 0x0F00;
    for (i, &to) in table.iter().enumerate() {
        if face & (0x8000 >> i) != 0 {
            out |= to;
        }
    }
    out
}

/// What the Option screen sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// `D_80100388` (= `D_80100358`): sound-effect volume, 0–0x7F.
    pub se_volume: i16,
    /// `D_80103690` (= `D_80103244`): CD (music) volume, 0–0x7F.
    pub cd_volume: i16,
    /// `D_80100374[pad]`: button layout 0–2.
    pub layout: [u8; 2],
}

impl Default for Settings {
    fn default() -> Self {
        Settings { se_volume: DEFAULT_VOLUME, cd_volume: DEFAULT_VOLUME, layout: [0; 2] }
    }
}

/// Raw primitives the Option pages add themselves (screen pixels; the original's are relative to
/// the (160, 120) draw offset).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionDraw {
    /// `func_8003ED18`: layer-1 TILEs. Pale (0x80, 0x80, 0xFF) half-transparent bars
    /// [`BAR_W`] wide, with blue (0, 0, 0xFF) bars as wide as the volumes on top, at
    /// [`BAR_X`], [`BAR_Y`]`[item]`, [`BAR_H`] tall.
    Sound { se: i16, cd: i16 },
    /// `func_8003F044` + `func_8003F2DC`: per pad, the layout number (OPTION page, u =
    /// 0x40 + 16·layout, v = 0x30, 16×16, palette 0) at ([`NUMBER_X`]`[pad]`, `number_y`), and
    /// the eight button labels ([`labels`]).
    Pads { layout: [u8; 2], number_y: i32 },
}

/// The volume bars (`D_800C3408`): x, y of each item, and size.
pub const BAR_X: i32 = 160 - 0x20;
pub const BAR_Y: [i32; 2] = [120 - 0x14, 120 + 0x46];
pub const BAR_W: i32 = 0x80;
pub const BAR_H: i32 = 0x10;
/// The layout numbers (`D_8010AEA8`): x per pad; y starts off-screen and slides 4 px a frame.
pub const NUMBER_X: [i32; 2] = [160 - 0x4C, 160 + 0x54];
const NUMBER_Y0: i32 = 120 - 0x86;
const NUMBER_STEP: i32 = 4;
/// The OPTION page's label palette (`GetClut(0, 501)`: row 2 of OPTION.TIM's CLUT).
pub const LABEL_CLUT: i16 = 2;
/// `D_800577AC`: each button label's (u, v, w, h) on the OPTION page.
pub const LABEL_UVWH: [(u8, u8, u8, u8); 8] = [
    (128, 48, 102, 12),
    (128, 60, 56, 12),
    (128, 72, 72, 12),
    (128, 112, 104, 8),
    (128, 84, 32, 12),
    (128, 120, 88, 8),
    (128, 96, 127, 12),
    (128, 112, 104, 8),
];
/// `D_8005777C[layout]`: each label's y.
pub const LABEL_Y: [[i32; 8]; 3] = [
    [198, 166, 128, 140, 88, 92, 100, 112],
    [102, 166, 128, 140, 184, 188, 196, 208],
    [102, 134, 160, 172, 184, 188, 196, 208],
];

/// Screen x of `pad`'s label `i` (`func_8003F400`: label 5 sits 0x28 further right).
pub fn label_x(pad: usize, i: usize) -> i32 {
    let x = if pad == 0 { 160 - 0x8C } else { 160 + 0x14 };
    if i == 5 {
        x + 0x28
    } else {
        x
    }
}

/// `D_80100368` on the list and the Sound page: arrow y of each item.
const LIST_Y: [i32; 2] = [0x60, 0xA0];
const SOUND_Y: [i32; 2] = [0x3C, 0x96];
/// `D_800B1344`: the arrow's x (off-screen on the Controller page).
const ARROW_X: i32 = 0x50;
const PADS_ARROW_X: i32 = 0x140;
/// `func_8003EBF4`: frames before ←/→ starts repeating.
const REPEAT_DELAY: u8 = 0x1E;

/// Option-screen state that isn't one of [`Front`]'s shared fields.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OptionState {
    /// `D_800C2ECA`.
    phase: u8,
    /// `D_800B1282`: the page the list picked (0 = back to the menu).
    pick: u8,
    /// `D_80100368[0..2]`.
    ys: [i32; 2],
    /// `D_800B1400[pad]` on the Controller page.
    pads: [i8; 2],
    /// `D_800C2F5C[pad]`: ←/→ auto-repeat counters.
    repeat: [u8; 2],
    /// `D_8010AEB2`: the layout numbers' y.
    number_y: i32,
}

impl Front {
    /// The menu's exit into mode 17 (`func_80027A64` state 1): script 0x15F.
    pub(super) fn to_option(&mut self) {
        self.pending = Some(script::LIST_IN);
        self.palettes[0] = 3;
        self.palettes[1] = 0;
        self.palettes[2] = 0;
        for s in 3..=5 {
            self.palettes[s] = 2;
        }
        self.set_mode(mode::OPTION);
    }

    /// `func_8003DDD8`.
    pub(super) fn option(&mut self, held: [u16; 2], pressed: [u16; 2]) {
        if self.sub == 0 {
            self.sub = 1;
            self.cursor = 0;
            self.opt.ys = LIST_Y;
            self.arrow = (ARROW_X, LIST_Y[0]);
            self.state = 0;
            self.opt.pick = 0;
            self.t = 0;
            self.opt.phase = 0;
            self.menu_state = 0;
            self.arrow_frame = ARROW_REST;
            self.number_init();
        }
        self.t += 1;
        match self.state {
            0 => {
                self.option_list(pressed);
                self.arrow.1 = self.opt.ys[self.cursor as usize & 1];
            }
            1 => {
                self.option_sound(held, pressed);
                self.option = Some(OptionDraw::Sound { se: self.settings.se_volume, cd: self.settings.cd_volume });
                self.arrow.1 = self.opt.ys[self.cursor as usize & 1];
            }
            _ => {
                self.option_pads(pressed);
                self.option = Some(OptionDraw::Pads { layout: self.settings.layout, number_y: self.opt.number_y });
            }
        }
        // Every page (and the frame that goes back to the menu) runs the scripts, the
        // background and the arrow.
        self.scripts_and_bg();
        self.menu = Some(MenuDraw { arrow_frame: self.arrow_frame, arrow: self.arrow, description: None });
    }

    /// `func_8003F140`: the layout numbers start above the screen.
    fn number_init(&mut self) {
        self.opt.number_y = NUMBER_Y0;
    }

    /// Page 0, the list.
    fn option_list(&mut self, pressed: [u16; 2]) {
        match self.opt.phase {
            0 => {
                if self.t == 1 {
                    self.opt.ys = LIST_Y;
                    self.arrow = (ARROW_X, LIST_Y[0]);
                    self.cursor = 0;
                    self.menu_state = 0;
                    self.arrow_frame = ARROW_REST;
                }
                if self.t >= 0x15 {
                    self.opt.phase = 1;
                }
            }
            1 => {
                if (pressed[0] | pressed[1]) & pad::TRIANGLE != 0 {
                    self.opt.phase = 2;
                    self.opt.pick = 0;
                    self.t = 0;
                    self.pending = Some(script::LIST_OUT);
                    return;
                }
                if self.menu_state == 0 {
                    // Pad 2 first; each compares its whole pressed word.
                    for p in [1, 0] {
                        if pressed[p] == pad::UP {
                            self.cursor -= 1;
                            self.sounds.push(0xA7);
                        }
                        if pressed[p] == pad::DOWN {
                            self.cursor += 1;
                            self.sounds.push(0xA7);
                        }
                        if self.cursor >= 2 {
                            self.cursor = 0;
                        }
                        if self.cursor < 0 {
                            self.cursor = 1;
                        }
                        if pressed[p] == pad::START || pressed[p] == pad::CROSS {
                            self.menu_state = 1;
                        }
                    }
                }
                if self.menu_state == 1 {
                    self.menu_state = 2;
                    self.sounds.push(0xA8);
                    match self.cursor {
                        0 => {
                            self.palettes[1] = 1;
                            self.opt.pick = 1;
                        }
                        1 => {
                            self.palettes[2] = 1;
                            self.opt.pick = 2;
                        }
                        _ => {}
                    }
                }
                if self.menu_state == 2 {
                    self.arrow_frame += 1;
                    if self.arrow_frame >= ARROW_END {
                        self.arrow_frame = ARROW_REST;
                        self.menu_state = 3;
                    }
                }
                if self.menu_state == 3 {
                    self.pending = Some(script::LIST_OUT);
                    self.t = 0;
                    self.opt.phase = 2;
                }
            }
            _ => {
                if self.t < 0x15 {
                    self.arrow.0 -= 16;
                    self.arrow.1 -= 2;
                    return;
                }
                match self.opt.pick {
                    0 => {
                        // Back to the mode menu (mode 8, script 0x105).
                        self.pending = Some(0x05);
                        self.palettes[0] = 2;
                        for s in [1, 2, 3, 4, 6] {
                            self.palettes[s] = 0;
                        }
                        self.palettes[5] = 7;
                        self.palettes[7] = 2;
                        self.palettes[8] = 2;
                        self.mode = mode::MENU;
                        self.sub = 0;
                    }
                    1 => {
                        self.palettes[1] = 0;
                        self.t = 0;
                        self.exit_len = 0x3C;
                    }
                    _ => {
                        self.t = 0;
                        self.palettes[2] = 0;
                        self.exit_len = 0x3C;
                    }
                }
                self.opt.phase = 0;
                self.state = self.opt.pick;
            }
        }
    }

    /// `func_8003EBF4(pad)`: → or ← on the frame it's pressed, then after 31 frames every other
    /// frame while it's the only button held.
    fn repeat_lr(&mut self, p: usize, held: u16, pressed: u16) -> u16 {
        let mut out = 0;
        for dir in [pad::RIGHT, pad::LEFT] {
            if pressed == dir {
                self.opt.repeat[p] = REPEAT_DELAY;
                out = dir;
            }
        }
        if pressed == 0 {
            for dir in [pad::RIGHT, pad::LEFT] {
                if held == dir {
                    self.opt.repeat[p] = self.opt.repeat[p].wrapping_sub(1);
                    if self.opt.repeat[p] == 0xFF {
                        self.opt.repeat[p] = 1;
                        out = dir;
                    }
                }
            }
        }
        out
    }

    /// Page 1, Sound.
    fn option_sound(&mut self, held: [u16; 2], pressed: [u16; 2]) {
        match self.opt.phase {
            0 => {
                if self.t == 1 {
                    self.opt.ys = SOUND_Y;
                    self.arrow = (ARROW_X, SOUND_Y[0]);
                    self.arrow_frame = ARROW_REST;
                    self.palettes[0] = 3;
                    self.palettes[3] = 2;
                    self.palettes[4] = 2;
                    self.cursor = 0;
                    self.menu_state = 0;
                    for s in [1, 2, 5, 6, 7, 8] {
                        self.palettes[s] = 0;
                    }
                    self.pending = Some(script::SOUND_IN);
                }
                if self.t >= 0x15 {
                    self.opt.phase = 1;
                }
            }
            1 => {
                let any = pressed[0] | pressed[1];
                if any & pad::TRIANGLE != 0 {
                    self.opt.phase = 2;
                    self.t = 0;
                    self.pending = Some(script::SOUND_OUT);
                }
                if any == pad::UP {
                    self.cursor -= 1;
                    self.sounds.push(0xA7);
                }
                if any == pad::DOWN {
                    self.cursor += 1;
                    self.sounds.push(0xA7);
                }
                if self.cursor >= 2 {
                    self.cursor = 0;
                } else if self.cursor < 0 {
                    self.cursor = 1;
                }
                let lr = self.repeat_lr(0, held[0], pressed[0]) | self.repeat_lr(1, held[1], pressed[1]);
                let vol = if self.cursor == 0 { &mut self.settings.se_volume } else { &mut self.settings.cd_volume };
                if lr == pad::RIGHT && *vol < MAX_VOLUME {
                    *vol += 1;
                    self.sounds.push(0xA7);
                } else if lr == pad::LEFT && *vol > 0 {
                    *vol -= 1;
                    self.sounds.push(0xA7);
                }
            }
            _ => {
                if self.t < 0x15 {
                    self.arrow.0 -= 16;
                    self.arrow.1 -= 2;
                } else {
                    self.back_to_list();
                }
            }
        }
    }

    /// The pages' exit: script 0x15F and the list again.
    fn back_to_list(&mut self) {
        self.pending = Some(script::LIST_IN);
        self.palettes[0] = 3;
        self.palettes[1] = 0;
        self.palettes[2] = 0;
        self.t = 0;
        self.opt.phase = 0;
        self.state = 0;
    }

    /// Page 2, Controller.
    fn option_pads(&mut self, pressed: [u16; 2]) {
        match self.opt.phase {
            0 => {
                if self.t == 1 {
                    self.arrow.0 = PADS_ARROW_X;
                    self.pending = Some(script::PADS_IN);
                    self.palettes[0] = 3;
                    for s in 1..=6 {
                        self.palettes[s] = 2;
                    }
                    for s in 7..=9 {
                        self.palettes[s] = 0;
                    }
                    self.opt.pads = self.settings.layout.map(|l| l as i8);
                    self.number_init();
                } else if self.t >= 0x14 {
                    self.opt.phase = 1;
                }
                self.opt.number_y += NUMBER_STEP;
            }
            1 => {
                if (pressed[0] | pressed[1]) & pad::TRIANGLE != 0 {
                    self.opt.phase = 2;
                    self.t = 0;
                    self.pending = Some(script::PADS_OUT);
                }
                for p in 0..2 {
                    if pressed[p] == pad::RIGHT {
                        self.opt.pads[p] += 1;
                        self.sounds.push(0xA7);
                    } else if pressed[p] == pad::LEFT {
                        self.opt.pads[p] -= 1;
                        self.sounds.push(0xA7);
                    }
                    if self.opt.pads[p] >= LAYOUTS as i8 {
                        self.opt.pads[p] = 0;
                    } else if self.opt.pads[p] < 0 {
                        self.opt.pads[p] = LAYOUTS as i8 - 1;
                    }
                    self.settings.layout[p] = self.opt.pads[p] as u8;
                }
            }
            _ => {
                if self.t < 0x15 {
                    self.opt.number_y -= NUMBER_STEP;
                } else {
                    self.back_to_list();
                    self.palettes[7] = 2;
                    self.palettes[8] = 2;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_swap_face_buttons() {
        let all = pad::SQUARE | pad::CROSS | pad::CIRCLE | pad::TRIANGLE;
        assert_eq!(remap(pad::CROSS | pad::START, 0), pad::CROSS | pad::START);
        assert_eq!(remap(pad::SQUARE, 1), pad::CROSS);
        assert_eq!(remap(pad::CROSS, 1), pad::SQUARE);
        assert_eq!(remap(pad::CIRCLE, 1), pad::CIRCLE);
        assert_eq!(remap(pad::CIRCLE, 2), pad::TRIANGLE);
        assert_eq!(remap(pad::TRIANGLE | pad::LEFT, 2), pad::CIRCLE | pad::LEFT);
        for l in 0..LAYOUTS {
            assert_eq!(remap(all, l), all);
        }
    }

    fn run(f: &mut Front, frames: usize, pads: [u16; 2]) {
        for _ in 0..frames {
            f.step2(pads);
        }
    }

    fn tap(f: &mut Front, b: u16) {
        f.step2([b, 0]);
        f.step2([0, 0]);
    }

    #[test]
    fn sound_page_steps_the_effect_volume_with_repeat() {
        let mut f = Front::new();
        f.mode = mode::OPTION;
        f.to_option();
        run(&mut f, 0x16, [0; 2]);
        tap(&mut f, pad::CROSS);
        // Arrow animation (26 frames), the list leaves (0x15), then the Sound page comes in.
        run(&mut f, 0x60, [0; 2]);
        assert_eq!(f.state, 1);
        assert_eq!(f.opt.phase, 1);
        assert_eq!(f.pending, None);
        f.step2([pad::RIGHT, 0]);
        assert_eq!(f.settings.se_volume, DEFAULT_VOLUME + 1);
        // Held: nothing for 30 frames, then a step on frame 31 and every second frame after.
        run(&mut f, 30, [pad::RIGHT, 0]);
        assert_eq!(f.settings.se_volume, DEFAULT_VOLUME + 1);
        run(&mut f, 1, [pad::RIGHT, 0]);
        assert_eq!(f.settings.se_volume, DEFAULT_VOLUME + 2);
        run(&mut f, 4, [pad::RIGHT, 0]);
        assert_eq!(f.settings.se_volume, DEFAULT_VOLUME + 4);
        assert_eq!(f.settings.cd_volume, DEFAULT_VOLUME);
        // △ goes back to the list.
        run(&mut f, 1, [0; 2]);
        tap(&mut f, pad::TRIANGLE);
        run(&mut f, 0x16, [0; 2]);
        assert_eq!(f.state, 0);
    }

    #[test]
    fn controller_page_cycles_each_pads_layout() {
        let mut f = Front::new();
        f.mode = mode::OPTION;
        f.to_option();
        run(&mut f, 0x16, [0; 2]);
        tap(&mut f, pad::DOWN);
        tap(&mut f, pad::CROSS);
        run(&mut f, 0x60, [0; 2]);
        assert_eq!(f.state, 2);
        assert_eq!(f.opt.number_y, NUMBER_Y0 + 20 * NUMBER_STEP);
        tap(&mut f, pad::LEFT);
        assert_eq!(f.settings.layout, [2, 0]);
        f.step2([0, pad::RIGHT]);
        assert_eq!(f.settings.layout, [2, 1]);
        f.step2([pad::TRIANGLE, 0]);
        run(&mut f, 0x15, [0; 2]);
        assert_eq!(f.state, 0);
        assert_eq!(f.opt.number_y, NUMBER_Y0);
        // △ on the list: back to the mode menu, settings kept.
        run(&mut f, 0x16, [0; 2]);
        tap(&mut f, pad::TRIANGLE);
        run(&mut f, 0x16, [0; 2]);
        assert_eq!(f.mode, mode::MENU);
        assert_eq!(f.settings.layout, [2, 1]);
    }
}
