//! `DATA/SD_CHR.OBJ`: the "SD" sprite scripts (result emblems, countdowns, messages).
//!
//! The file is loaded at RAM `0x801A8000` (file index 1, `func_80017744`) and is all absolute
//! pointers into itself. Word *n* points to script *n*: a list of 16-byte object records
//! `(i32 layer, i32 flip, u32 anim, u32 motion)`, ended by `layer ≤ 0`. Starting a script
//! (`func_8002EE6C`) gives record *i* to object slot *i* of 25 (`D_8005ECA4`, 0x48 bytes each)
//! and kills the rest.
//!
//! Every frame each live object runs its motion script (`func_8002F094`), then its animation
//! script (`func_8002F5B0`), then is drawn (`func_80030070`).
//!
//! - Motion: words; the low byte of the first is the opcode (`jtbl_80011E50`). 1 move to
//!   `(x, y)` in *n* frames, 2 wait *n*, 3 stop, 4 set position, 5 wait *n* and step by
//!   `(dx, dy)`, 6 jump, 7 move by `(dx, dy)` in *n* frames. Ops run while the wait counter is 0.
//!   Positions are 16.16 (`+0`/`+6` x, `+4`/`+2` y), velocities 8.8 px per frame.
//! - Animation: `(u16 frames, u32 frame_ptr)` pairs as in `BLT_ANM.OBJ`. A first word of `0xFFFF`
//!   is a command, sub-op at `+4` (`jtbl_80011E70`): 0 jump, 1 kill, 2 set layer, 4 scale tween
//!   `(n, sx, sy)` (20.12 fixed point), 5 sound effect.
//! - Frames are the 9 × i16 records of [`Frame`].

use crate::anm::Frame;
use crate::{u16le, u32le, Error};

pub const BASE: u32 = 0x801A_8000;
pub const SLOTS: usize = 25;
/// Bound on script steps per object per frame (the original loops until a wait is set).
const MAX_OPS: usize = 64;

#[derive(Clone, Debug)]
pub struct SdBank {
    data: Vec<u8>,
}

impl SdBank {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let bank = SdBank { data: data.to_vec() };
        let first = bank.u32(BASE).ok_or(Error::Truncated)?;
        if !bank.contains(first) {
            return Err(Error::BadMagic("SD_CHR script table"));
        }
        Ok(bank)
    }

    fn contains(&self, addr: u32) -> bool {
        addr >= BASE && ((addr - BASE) as usize) < self.data.len()
    }

    fn off(&self, addr: u32) -> Option<usize> {
        self.contains(addr).then(|| (addr - BASE) as usize)
    }

    fn u32(&self, addr: u32) -> Option<u32> {
        u32le(&self.data, self.off(addr)?).ok()
    }

    fn i32(&self, addr: u32) -> Option<i32> {
        self.u32(addr).map(|v| v as i32)
    }

    fn u16(&self, addr: u32) -> Option<u16> {
        u16le(&self.data, self.off(addr)?).ok()
    }

    fn byte(&self, addr: u32) -> Option<u8> {
        self.data.get(self.off(addr)?).copied()
    }

    pub fn frame(&self, ptr: u32) -> Option<Frame> {
        Frame::from_bytes(self.data.get(self.off(ptr)?..)?)
    }

    /// Address of script `id`'s object records.
    pub fn script(&self, id: u8) -> Option<u32> {
        self.u32(BASE + 4 * u32::from(id)).filter(|&p| self.contains(p))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SdObject {
    /// `+8`: OT layer; negative = dead.
    pub layer: i32,
    /// `+0xC`: drawn mirrored when this differs from the frame's own flip.
    pub flip: i32,
    pub x: i16,
    x_frac: u16,
    pub y: i16,
    y_frac: u16,
    vx: i16,
    vy: i16,
    /// `+0x44`: when 0, motion op 5 negates its dx.
    mirror: i16,
    wait: i16,
    motion: u32,
    anim_wait: i16,
    anim: u32,
    pub frame: Option<Frame>,
    /// `+0x24`/`+0x26`, out of 128.
    pub scale: [i16; 2],
    scale_fx: [i32; 2],
    scale_step: [i32; 2],
    scale_count: i16,
    /// Live at the start of the last step (a kill still draws that frame).
    pub visible: bool,
}

impl SdObject {
    pub fn alive(&self) -> bool {
        self.layer >= 0
    }
}

/// The 25 object slots.
#[derive(Clone, Debug)]
pub struct SdStage {
    pub objects: [SdObject; SLOTS],
    /// `D_800699DC`: set once any script has started.
    pub active: bool,
}

impl Default for SdStage {
    fn default() -> Self {
        SdStage { objects: [SdObject { layer: -1, ..Default::default() }; SLOTS], active: false }
    }
}

enum Ctl {
    Continue,
    Stop,
}

impl SdStage {
    /// `func_8002EE6C`: start script `id`. Unknown ids kill everything.
    pub fn start(&mut self, bank: &SdBank, id: u8) {
        let mut rec = bank.script(id);
        self.active = true;
        for obj in &mut self.objects {
            let layer = rec.and_then(|r| bank.i32(r)).unwrap_or(0);
            if layer <= 0 {
                rec = None;
                obj.layer = -1;
                continue;
            }
            let r = rec.unwrap();
            obj.layer = layer;
            obj.flip = bank.i32(r + 4).unwrap_or(0);
            obj.anim = bank.u32(r + 8).unwrap_or(0);
            obj.motion = bank.u32(r + 12).unwrap_or(0);
            obj.scale = [0x80; 2];
            obj.scale_count = 0;
            obj.anim_wait = 0;
            obj.wait = 0;
            rec = Some(r + 16);
        }
    }

    /// One frame for every live object; returns the sound effects its scripts triggered.
    pub fn step(&mut self, bank: &SdBank) -> Vec<u8> {
        let mut sounds = Vec::new();
        for obj in &mut self.objects {
            obj.visible = obj.alive();
            if obj.visible {
                motion(obj, bank);
                anim(obj, bank, &mut sounds);
            }
        }
        sounds
    }

    /// Objects to draw this frame, in slot order.
    pub fn visible(&self) -> impl Iterator<Item = &SdObject> {
        self.objects.iter().filter(|o| o.visible && o.frame.is_some())
    }
}

/// `func_8002F094`.
fn motion(o: &mut SdObject, bank: &SdBank) {
    let was = o.wait;
    if was == 0 {
        let mut p = o.motion;
        for _ in 0..MAX_OPS {
            p = motion_op(o, bank, p);
            if o.wait != 0 {
                break;
            }
        }
        o.motion = p;
    }
    if was >= 0 {
        o.wait = o.wait.wrapping_sub(1);
    }
    let x = ((i32::from(o.x) << 16) | i32::from(o.x_frac)).wrapping_add(i32::from(o.vx) << 8);
    let y = ((i32::from(o.y) << 16) | i32::from(o.y_frac)).wrapping_add(i32::from(o.vy) << 8);
    (o.x, o.x_frac, o.y, o.y_frac) = ((x >> 16) as i16, x as u16, (y >> 16) as i16, y as u16);
}

/// `func_8002F1A8`: one motion op at `p`; returns the next op's address.
fn motion_op(o: &mut SdObject, bank: &SdBank, p: u32) -> u32 {
    let arg = |i: u32| bank.i32(p + 4 + 4 * i).unwrap_or(0);
    let Some(op) = bank.byte(p) else {
        o.wait = -1;
        return p;
    };
    match op {
        1 | 7 => {
            // Move to (1) or by (7) (x, y) over n frames.
            let n = arg(0);
            o.wait = n as i16;
            if n == 0 {
                // func_800324B4, not used by the ported scripts.
                return p + 4;
            }
            let (dx, dy) = if op == 1 {
                (arg(1) - i32::from(o.x), arg(2) - i32::from(o.y))
            } else {
                (arg(1), arg(2))
            };
            o.vx = ((dx << 8) / n) as i16;
            o.vy = ((dy << 8) / n) as i16;
            o.x_frac = 0;
            o.y_frac = 0;
            p + 16
        }
        2 => {
            o.vx = 0;
            o.vy = 0;
            o.wait = arg(0) as i16;
            p + 8
        }
        4 => {
            o.vx = 0;
            o.vy = 0;
            o.x = arg(0) as i16;
            o.y = arg(1) as i16;
            p + 12
        }
        5 => {
            o.vx = 0;
            o.vy = 0;
            o.wait = arg(0) as i16;
            let dx = arg(1) as i16;
            let dx = if o.mirror == 0 { dx.wrapping_neg() } else { dx };
            o.x = o.x.wrapping_add(dx);
            o.y = o.y.wrapping_add(arg(2) as i16);
            p + 16
        }
        6 => bank.u32(p + 4).unwrap_or(p),
        // 3 stop; 0 (an endless loop in the original) and unknown ops stop too.
        _ => {
            o.vx = 0;
            o.vy = 0;
            o.wait = -1;
            p + 4
        }
    }
}

/// `func_8002F5B0`.
fn anim(o: &mut SdObject, bank: &SdBank, sounds: &mut Vec<u8>) {
    if o.scale_count != 0 {
        o.scale_count -= 1;
        for i in 0..2 {
            o.scale_fx[i] = o.scale_fx[i].wrapping_add(o.scale_step[i]);
            o.scale[i] = (o.scale_fx[i] / 4096) as i16;
        }
    }
    if o.anim_wait > 0 {
        o.anim_wait -= 1;
        return;
    }
    if o.anim_wait < 0 {
        return;
    }
    let mut p = o.anim;
    for _ in 0..MAX_OPS {
        let Some(word) = bank.u32(p) else {
            o.anim_wait = -1;
            break;
        };
        if word == 0xFFFF {
            let (next, ctl) = command(o, bank, p, sounds);
            p = next;
            if let Ctl::Stop = ctl {
                break;
            }
            continue;
        }
        o.anim_wait = bank.u16(p).unwrap_or(0).wrapping_sub(1) as i16;
        o.frame = bank.u32(p + 4).and_then(|f| bank.frame(f));
        p += 8;
        break;
    }
    o.anim = p;
}

/// `func_8002F6D4`: the command at `p` (its first word is `0xFFFF`).
fn command(o: &mut SdObject, bank: &SdBank, p: u32, sounds: &mut Vec<u8>) -> (u32, Ctl) {
    let arg = |i: u32| bank.i32(p + 8 + 4 * i).unwrap_or(0);
    match bank.u16(p + 4).unwrap_or(0xFFFF) {
        0 => (bank.u32(p + 8).unwrap_or(p), Ctl::Continue),
        1 => {
            o.layer = -1;
            (p + 8, Ctl::Stop)
        }
        2 => {
            o.layer = arg(0) as i16 as i32;
            (p + 12, Ctl::Continue)
        }
        4 => {
            let n = arg(0) as i16;
            let target = [arg(1) as u16 as i32, arg(2) as u16 as i32];
            o.scale_count = n;
            for i in 0..2 {
                if n != 0 {
                    o.scale_step[i] = ((target[i] - i32::from(o.scale[i])) << 12) / i32::from(n);
                } else {
                    o.scale_fx[i] = target[i] << 12;
                    o.scale[i] = target[i] as i16;
                }
            }
            (p + 20, Ctl::Continue)
        }
        5 => {
            sounds.push(arg(0) as u8);
            (p + 12, Ctl::Continue)
        }
        // 3 and 6 loop forever in the original; 7 (`func_800302B0`) is not ported.
        _ => {
            o.anim_wait = -1;
            (p, Ctl::Stop)
        }
    }
}

/// Screen rectangle of an object the way `func_80030070` (mode 2) and `func_8001C414` place
/// it: scaled about its bottom centre, `(x0, y0, w, h)` in screen pixels, plus whether it is
/// mirrored.
pub fn placement(o: &SdObject, f: &Frame) -> (i32, i32, i32, i32, bool) {
    let [sx, sy] = o.scale.map(i32::from);
    let (w, h) = (i32::from(f.w), i32::from(f.h));
    let grow_x = ((w * sx) / 128 - w) / 2;
    let grow_y = (h * sy) / 128 - h;
    let x = i32::from(o.x.wrapping_sub(grow_x as i16));
    let y = i32::from(o.y.wrapping_sub(grow_y as i16));
    let x0 = x + ((i32::from(f.dx) * sx) >> 7);
    let y0 = y + ((i32::from(f.dy) * sy) >> 7);
    let dw = (w * (sx + 2)) >> 7;
    let dh = (h * (sy + 2)) >> 7;
    (x0, y0, dw, dh, o.flip != i32::from(f.flip))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny bank: script 0 = one object that sets its position, moves 20 px down in 4 frames
    /// and stops, showing a 2-frame loop.
    fn bank() -> SdBank {
        let mut d = vec![0u8; 0x200];
        let mut w = |at: usize, v: u32| d[at..at + 4].copy_from_slice(&v.to_le_bytes());
        w(0x00, BASE + 0x10); // script 0
        // record: layer 1, flip 0, anim 0x40, motion 0x80; then terminator
        w(0x10, 1);
        w(0x14, 0);
        w(0x18, BASE + 0x40);
        w(0x1C, BASE + 0x80);
        w(0x20, 0);
        // anim: 3×frame A, 2×frame B, jump back
        w(0x40, 3);
        w(0x44, BASE + 0x100);
        w(0x48, 2);
        w(0x4C, BASE + 0x120);
        w(0x50, 0xFFFF);
        w(0x54, 0);
        w(0x58, BASE + 0x40);
        // motion: setpos (100, 50); moveto n=4 (100, 70); stop
        w(0x80, 4);
        w(0x84, 100);
        w(0x88, 50);
        w(0x8C, 1);
        w(0x90, 4);
        w(0x94, 100);
        w(0x98, 70);
        w(0x9C, 3);
        // frames: w=8 h=8, u differs
        w(0x104, 8 | 8 << 16);
        w(0x120, 16);
        w(0x124, 8 | 8 << 16);
        SdBank { data: d }
    }

    #[test]
    fn moves_then_stops_and_loops_its_frames() {
        let b = bank();
        let mut s = SdStage::default();
        s.start(&b, 0);
        let mut ys = vec![];
        let mut us = vec![];
        for _ in 0..8 {
            s.step(&b);
            let o = &s.objects[0];
            ys.push(o.y);
            us.push(o.frame.unwrap().u);
        }
        assert_eq!(ys, [55, 60, 65, 70, 70, 70, 70, 70]);
        assert_eq!(us, [0, 0, 0, 16, 16, 0, 0, 0]);
        assert!(!s.objects[1].alive());
    }

    #[test]
    fn scale_grows_from_the_bottom_centre() {
        let o = SdObject { x: 100, y: 64, scale: [64, 64], ..Default::default() };
        let f = Frame { w: 120, h: 64, ..Default::default() };
        // Half size, centred on x 160 with the bottom at 128; the quad is (s + 2)/128 wide.
        assert_eq!(placement(&o, &f), (130, 96, 61, 33, false));
    }
}
