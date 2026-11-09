//! Mode 5, the Arcade screens (`func_80015F28`, sub-states `D_800C34AA` via `jtbl_800100F0`).
//!
//! - 0/1 (`func_8002840C`, `func_8002855C`): "Player 1 vs CPU" (→ mode 13) or "Player 1 vs
//!   Player 2 / Set Match" (→ mode 14), with ←/→ on the second item picking 1, 3 or 5 sets.
//! - 6/7 (`func_800290F4`, `func_8002922C`): the vs-CPU course list. Not ported: mode 13 needs
//!   the CPU player, so confirming "vs CPU" comes back to sub-state 0 here.
//! - 8/9 (`func_8002A644`, `func_8002A784`): both players pick a difficulty (the five level
//!   names) with their own arrow, and ←/→ sets their □ stock (0–3). When both have confirmed,
//!   mode 9 hands over to mode 14.

use super::{mode, Arrow, Front, Music, Overlay, ARROW_END, ARROW_REST, ARROW_Y};
use crate::game::pad;

/// SD_CHR scripts of the Arcade screens (requested as `0x100 | id`).
pub mod script {
    /// The two Arcade items come in.
    pub const MODES_IN: u8 = 0x08;
    /// They leave after a choice.
    pub const MODES_OUT: u8 = 0x09;
    /// △: they leave, back to the mode menu.
    pub const MODES_BACK: u8 = 0x0A;
    /// The difficulty list comes in (the same list as Time Trial's courses).
    pub const LEVELS_IN: u8 = 0x0B;
    /// Both players confirmed.
    pub const LEVELS_OUT: u8 = 0x0C;
    /// △ with nobody confirmed: back to the Arcade items.
    pub const LEVELS_BACK: u8 = 0x0D;
    /// The vs-CPU course list (sub-state 6).
    pub const COURSES_IN: u8 = 0x0E;
}

/// `D_80100368` on the Arcade items: arrow y of "vs CPU" and "vs Player 2".
const ITEM_Y: [i32; 2] = [0x53, 0x83];
/// `D_800B1344` on the Arcade items (the arrow's x).
const ITEM_ARROW_X: i32 = 0x48;
/// `D_8010ADB0`: sets for each set-match choice (`D_800C2FE6`).
pub const SETS: [u8; 3] = [1, 3, 5];
/// Where the set-match digit `D_80056A18[choice]` is drawn (layer 0).
pub const SETS_AT: (i32, i32) = (0x90, 0x98);
/// `D_800B0050` when nobody is the CPU (mode 14).
pub const NO_CPU: u8 = 0xFF;
/// Arrows on the difficulty list: player 1 at the left, player 2 at the right, mirrored.
const LEVEL_ARROW_X: [i32; 2] = [0x28, 0xD8];
/// Difficulties on the list (`D_80102740` = cursor + 1).
const LEVELS: i8 = 5;
/// Highest □ stock (`D_800FFCF8`).
pub const MAX_STOCK: u8 = 3;
/// `func_8001D778(0x40, 0xF0, 0x30)`: the stock labels' x for each player, and their y.
pub const STOCK_X: [i32; 2] = [0x40, 0xF0];
pub const STOCK_Y: i32 = 0x30;

/// One player's arrow on the difficulty list (`D_800B1400[p]`, `D_800B1340 + 0x24·p` …).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Pick {
    pub cursor: i8,
    /// 0 choosing, 1 confirmed, 2 arrow plays, 3 done.
    pub state: u8,
    pub frame: usize,
    pub at: (i32, i32),
}

impl Front {
    /// The menu's Arcade exit (`func_80027A64`) and every "back" into sub-state 0: script
    /// 0x108, palettes slot 0 = 3, 3 = 7, 1–2 = 0, 4–6 = 2.
    pub(super) fn to_arcade(&mut self) {
        self.pending = Some(script::MODES_IN);
        self.brightness = 0x80;
        self.palettes[0] = 3;
        self.palettes[3] = 7;
        self.palettes[1] = 0;
        self.palettes[2] = 0;
        for s in 4..=6 {
            self.palettes[s] = 2;
        }
        self.state = 0;
        self.set_mode(mode::ARCADE);
    }

    pub(super) fn arcade(&mut self, held: [u16; 2], pressed: [u16; 2]) {
        let _ = held;
        match self.sub {
            0 => self.arcade_init(),
            1 => self.arcade_items(pressed[0]),
            8 => self.levels_init(),
            9 => self.levels(pressed),
            _ => {}
        }
    }

    /// `func_8002840C`.
    fn arcade_init(&mut self) {
        self.sub = 1;
        self.state = 0;
        self.t = 0;
        self.brightness = 0x80;
        self.cursor = 0;
        self.set_match = 0;
        self.arrow = (ITEM_ARROW_X, ITEM_Y[0]);
        self.arrow_frame = ARROW_REST;
        self.menu_state = 0;
        self.sets = SETS[0];
        self.scripts_and_bg();
    }

    fn item_arrow(&mut self) {
        self.arrows.push(Arrow { frame: self.arrow_frame, at: self.arrow, flip: false });
    }

    /// `func_8002855C`. Only pad 1 drives it (the menu's pad `D_800C2FB8`).
    fn arcade_items(&mut self, pressed: u16) {
        self.t += 1;
        match self.state {
            0 => {
                if self.t == 0x15 {
                    self.overlays.push(Overlay::Sets { choice: self.set_match as usize, at: SETS_AT });
                }
                if self.t >= 0x14 {
                    self.t = 0x14;
                    if pressed == pad::TRIANGLE {
                        self.state = 2;
                        self.brightness = 0x80;
                        self.pending = Some(script::MODES_BACK);
                        self.palettes[0] = 3;
                        self.palettes[3] = 7;
                        self.t = 0;
                        self.palettes[1] = 0;
                        self.palettes[2] = 0;
                        for s in 4..=6 {
                            self.palettes[s] = 2;
                        }
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
                    if self.cursor >= 2 {
                        self.cursor = 0;
                    }
                    if self.cursor < 0 {
                        self.cursor = 1;
                    }
                    if pressed == pad::START || pressed == pad::CROSS {
                        self.menu_state = 1;
                    }
                    if self.cursor == 1 {
                        if pressed == pad::RIGHT {
                            self.set_match += 1;
                            self.sounds.push(0xA7);
                        }
                        if pressed == pad::LEFT {
                            self.set_match -= 1;
                            self.sounds.push(0xA7);
                        }
                        if self.set_match >= 3 {
                            self.set_match = 0;
                        }
                        if self.set_match < 0 {
                            self.set_match = 2;
                        }
                        self.sets = SETS[self.set_match as usize];
                    }
                }
                if self.menu_state == 1 {
                    self.menu_state = 2;
                    self.sounds.push(0xA8);
                    match self.cursor {
                        0 => {
                            // The human is on the menu's pad, the CPU on the other side.
                            self.cpu = 1;
                            self.palettes[1] = 1;
                            self.choice = mode::VS_CPU;
                        }
                        _ => {
                            self.cpu = NO_CPU;
                            self.palettes[2] = 1;
                            self.choice = mode::VERSUS;
                        }
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
                    self.state = 1;
                    self.t = 0;
                    self.pending = Some(script::MODES_OUT);
                }
                self.arrow.1 = ITEM_Y[self.cursor as usize];
                self.item_arrow();
                self.scripts_and_bg();
            }
            // The items leave; △ comes back, otherwise the next screen after 0x3C frames.
            1 => {
                if self.t < 0x28 {
                    self.overlays.push(Overlay::Sets { choice: self.set_match as usize, at: SETS_AT });
                }
                if self.t >= 0x14 {
                    self.arrow.0 -= 16;
                    self.arrow.1 -= 2;
                }
                self.item_arrow();
                self.scripts_and_bg();
                if pressed == pad::TRIANGLE {
                    self.to_arcade();
                } else if self.t >= 0x3C {
                    self.brightness = 0x80;
                    self.state = 0;
                    self.t = 0;
                    self.exit_len = 0;
                    if self.cpu == NO_CPU {
                        self.sub = 8;
                        self.pending = Some(script::LEVELS_IN);
                        self.level_palettes();
                    } else {
                        // Sub-state 6 (script 0x10E, the vs-CPU courses) isn't ported.
                        self.to_arcade();
                    }
                }
            }
            // △: the arrow flies on, then the mode menu comes back.
            _ => {
                self.arrow.0 -= 16;
                self.arrow.1 -= 2;
                self.item_arrow();
                self.scripts_and_bg();
                if self.t >= 0x14 {
                    self.back_to_menu();
                }
            }
        }
    }

    /// Slot 0 = 3, 6 = 7, 1–5 = 0, 7–9 = 2: the difficulty list's palettes.
    fn level_palettes(&mut self) {
        self.palettes[0] = 3;
        self.palettes[6] = 7;
        for s in 1..=5 {
            self.palettes[s] = 0;
        }
        for s in 7..=9 {
            self.palettes[s] = 2;
        }
    }

    /// `func_8002A644`.
    fn levels_init(&mut self) {
        self.sub = 9;
        for (p, pick) in self.picks.iter_mut().enumerate() {
            *pick = Pick { cursor: 2, state: 0, frame: ARROW_REST, at: (LEVEL_ARROW_X[p], ARROW_Y[2]) };
        }
        self.stock = [0, 0];
        self.state = 0;
        self.t = 0;
        self.spin_scale = 0;
        self.scripts_and_bg();
    }

    fn level_arrows(&mut self) {
        for (p, pick) in self.picks.iter().enumerate() {
            self.arrows.push(Arrow { frame: pick.frame, at: pick.at, flip: p == 1 });
        }
    }

    /// `func_8002A784`: both pads at once.
    fn levels(&mut self, mut pressed: [u16; 2]) {
        self.t += 1;
        match self.state {
            0 => {
                // func_8001D778(0x40, 0xF0, 0x30) with D_80100674/6 = the stocks.
                for p in 0..2 {
                    self.overlays.push(Overlay::Stock { player: p, value: self.stock[p], x: STOCK_X[p] });
                }
                // △ on a confirmed player takes the choice back (and uses up the press).
                for p in 0..2 {
                    if self.picks[p].state == 3 && pressed[p] == pad::TRIANGLE {
                        self.picks[p].frame = ARROW_REST;
                        self.picks[p].state = 0;
                        let other = self.picks[1 - p];
                        if other.state == 0 || other.cursor != self.picks[p].cursor {
                            self.palettes[self.picks[p].cursor as usize + 1] = 0;
                        }
                        pressed[p] = 0;
                    }
                }
                for p in 0..2 {
                    let pr = pressed[p];
                    if self.picks[p].state == 0 {
                        let pick = &mut self.picks[p];
                        if pr == pad::UP {
                            pick.cursor -= 1;
                            self.sounds.push(0xA7);
                        }
                        if pr == pad::DOWN {
                            pick.cursor += 1;
                            self.sounds.push(0xA7);
                        }
                        if pick.cursor >= LEVELS {
                            pick.cursor = 0;
                        }
                        if pick.cursor < 0 {
                            pick.cursor = LEVELS - 1;
                        }
                        if pr == pad::RIGHT && self.stock[p] < MAX_STOCK {
                            self.stock[p] += 1;
                            self.sounds.push(0xA7);
                        }
                        if pr == pad::LEFT && self.stock[p] > 0 {
                            self.stock[p] -= 1;
                            self.sounds.push(0xA7);
                        }
                        if pr == pad::START || pr == pad::CROSS {
                            self.picks[p].state = 1;
                        }
                        if self.t >= 0x14 {
                            self.t = 0x14;
                            if self.picks[0].state == 0
                                && self.picks[1].state == 0
                                && (pressed[0] == pad::TRIANGLE || pressed[1] == pad::TRIANGLE)
                            {
                                self.brightness = 0x80;
                                self.pending = Some(script::LEVELS_BACK);
                                self.t = 0;
                                self.state = 2;
                                self.level_palettes();
                            }
                        }
                    }
                    if self.picks[p].state == 1 {
                        self.picks[p].state = 2;
                        self.sounds.push(0xA8);
                        let c = self.picks[p].cursor;
                        if (0..LEVELS).contains(&c) {
                            self.palettes[c as usize + 1] = 1;
                            self.difficulty[p] = c as u8 + 1;
                            self.level_row = 0;
                        }
                    }
                    if self.picks[p].state == 2 {
                        self.picks[p].frame += 1;
                        if self.picks[p].frame >= ARROW_END {
                            self.picks[p].frame = ARROW_REST;
                            self.picks[p].state = 3;
                        }
                    }
                    if self.picks[0].state == 3 && self.picks[1].state == 3 {
                        self.t = 0;
                        self.state = 1;
                        self.pending = Some(script::LEVELS_OUT);
                    }
                    self.picks[p].at.1 = ARROW_Y[self.picks[p].cursor as usize];
                }
                self.level_arrows();
                self.scripts_and_bg();
            }
            // Both confirmed: the arrows fly apart; △ from either pad starts over, otherwise
            // mode 9 after 0x50 frames.
            1 => {
                if self.t >= 0x14 {
                    self.fly_level_arrows();
                }
                self.level_arrows();
                self.scripts_and_bg();
                if pressed[0] == pad::TRIANGLE || pressed[1] == pad::TRIANGLE {
                    self.brightness = 0x80;
                    self.sub = 8;
                    self.pending = Some(script::LEVELS_IN);
                    self.palettes[0] = 3;
                    self.palettes[6] = 7;
                    self.state = 0;
                    self.t = 0;
                    self.exit_len = 0;
                    for s in 1..=5 {
                        self.palettes[s] = 0;
                    }
                    for s in 7..=9 {
                        self.palettes[s] = 2;
                    }
                } else if self.t >= 0x50 {
                    self.brightness = 0x80;
                    self.state = 0;
                    self.exit_len = 0;
                    self.music = Some(Music::Stop);
                    self.set_mode(mode::LAUNCH);
                }
            }
            // △: back to the Arcade items.
            _ => {
                self.fly_level_arrows();
                self.level_arrows();
                self.scripts_and_bg();
                if self.t >= 0x14 {
                    self.to_arcade();
                }
            }
        }
    }

    fn fly_level_arrows(&mut self) {
        self.picks[0].at.0 -= 16;
        self.picks[0].at.1 -= 1;
        self.picks[1].at.0 += 16;
        self.picks[1].at.1 -= 1;
    }
}
