//! `DATA/SD_CHR.OBJ`: the "SD" sprite scripts (result emblems, countdowns, messages).
//!
//! The file is loaded at RAM `0x801A8000` (file index 1, `func_80017744`) and is all absolute
//! pointers into itself. Word *n* points to script *n*: a list of 16-byte object records
//! `(i32 layer, i32 flip, u32 anim, u32 motion)`, ended by `layer ≤ 0`. Starting a script
//! (`func_8002EE6C`) gives record *i* to object slot *i* of 25 (`D_8005ECA4`, 0x48 bytes each)
//! and kills the rest. Request `0x2xx` (`func_8002EF44`, [`SdStage::start_placed`]) reads the
//! same script as 20-byte records with a start `(i16 x, i16 y)` appended.
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
//!   `(n, sx, sy)` (20.12 fixed point), 5 sound effect, 7 upload `VNAO1–3.TIM` (`func_800302B0`,
//!   see [`SdStage::vnao`]).
//! - Frames are the 9 × i16 records of [`Frame`].
//!
//! **Actors** (`func_80031910`, [`Vm::Actor`]) are a second copy of this VM for the mascots
//! beside the field: 4 objects per actor at `D_8005F3E4` (0x120 bytes per actor), started from
//! a per-mood record list ([`SdBank::actor_script`]) by `func_80031AC8`. Motion is the same
//! (no op 7); the animation commands (`jtbl_80011F0C`) differ: 1 raises the actor's "done" flag
//! (`D_8005F3D4`) and stops without killing the object, 7 uploads one of the character
//! sheets (`func_80032BCC`). Objects are drawn by `func_80032934` (see [`actor_placement`]).

use crate::anm::Frame;
use crate::{u16le, u32le, Error};

pub const BASE: u32 = 0x801A_8000;
pub const SLOTS: usize = 25;
/// Objects per actor.
pub const ACTOR_SLOTS: usize = 4;
/// Chain Reaction's mascot table (`D_801A8308`, read by `func_80031828`): a pointer to the
/// per-mood record lists. Other modes index `D_801A8200` by background set.
pub const CHAIN_ACTOR: u32 = 0x801A_8308;
/// Solo's mascot table (`D_801A8304`, read by `func_80031660`).
pub const SOLO_ACTOR: u32 = 0x801A_8304;
/// Mode 13's mascot tables (`D_801A8200[stage]`, read by `func_800319CC`): one word per
/// stage, each the mood list itself (pass `VS_ACTORS + 4·stage` to [`SdBank::actor_script`]).
pub const VS_ACTORS: u32 = 0x801A_8200;
/// Time Trial's mascot table (`D_801A8300`, read by `func_8003151C`).
pub const TRIAL_ACTOR: u32 = 0x801A_8300;
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

    /// Address of the actor records for `mood` in the table pointed to by `table` (e.g.
    /// [`CHAIN_ACTOR`]).
    pub fn actor_script(&self, table: u32, mood: u8) -> Option<u32> {
        let moods = self.u32(table).filter(|&p| self.contains(p))?;
        self.u32(moods + 4 * u32::from(mood)).filter(|&p| self.contains(p))
    }

    /// Cut-in object `object` (0–2) of `entry` (0–15): its `(frames, frame)` steps up to the
    /// end op. See [`CUT_IN_TABLES`].
    pub fn cut_in(&self, object: usize, entry: u8) -> Vec<(u32, Option<Frame>)> {
        let mut steps = Vec::new();
        let Some(&table) = CUT_IN_TABLES.get(object) else { return steps };
        let Some(mut p) = self.u32(table + 4 * u32::from(entry)) else { return steps };
        for _ in 0..256 {
            let Some(n) = self.u32(p) else { break };
            if n == 0xFFFF {
                // func_80039300 / func_8003956C: 0 jump, 1 end, 2 set +2 (one argument).
                match self.u32(p + 4) {
                    Some(0) => match self.u32(p + 8) {
                        Some(to) => p = to,
                        None => break,
                    },
                    Some(2) => p += 12,
                    _ => break,
                }
                continue;
            }
            let frame = self.u32(p + 4).and_then(|f| self.frame(f));
            steps.push((n, frame));
            p += 8;
        }
        steps
    }

    /// The frame cut-in object `object` of `entry` shows `age` frames after it started
    /// (`None` once its end op has run).
    pub fn cut_in_frame(&self, object: usize, entry: u8, age: u32) -> Option<Frame> {
        let mut t = 0;
        for (n, f) in self.cut_in(object, entry) {
            t += n;
            if age < t {
                return f;
            }
        }
        None
    }
}

/// The two-player cut-ins' script tables (`func_80039084`, `func_80039158`, `func_8003922C`),
/// 16 entries each: entry = the chain value (2–14) or 15 for a bomb. A script is
/// `(u32 frames, u32 frame_ptr)` pairs; `0xFFFF` starts a command (op 1 ends the object; only
/// that one is used). Object 0 is the "n Chain" text (RENSA2), 1 its backdrop, 2 the burst
/// (RENSA1), drawn by `func_80039708` and siblings at [`CUT_IN_AT`] with scale [`CUT_IN_SCALE`].
pub const CUT_IN_TABLES: [u32; 3] = [0x801A_8240, 0x801A_8280, 0x801A_82C0];
/// Where each cut-in object is drawn, per player (`func_80039708` … `func_80039960`).
pub const CUT_IN_AT: [[(i32, i32); 3]; 2] = [[(0x32, 0x28), (0x26, 0x78), (0x30, 0x1E)], [(0xDE, 0x28), (0xD2, 0x78), (0xDE, 0x1E)]];
/// `func_8001B148` scale of each cut-in object (out of 128).
pub const CUT_IN_SCALE: [i32; 3] = [0xA0, 0xA0, 0x190];

/// Which copy of the VM a stage runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Vm {
    /// SD_CHR scripts (`func_8002F094`, `func_8002F5B0`).
    #[default]
    Sd,
    /// Mascot actors (`func_80032180`, `func_80032590`).
    Actor,
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
    /// Which RAM slot (1–4, 0 = none yet; `GRAPH/VNAOn.TIM` in solo and mode 7, the stage's
    /// character sheets in vs CPU) texture pages 26 and 27 show. Anim command 7
    /// uploads one (they share a VRAM spot, so the last upload wins): argument 0 → VNAO1 as
    /// page 26, 1 → VNAO2 as 26, 2 → VNAO3 as 27, 3 → VNAO3 as 26 (`func_800302B0`; the files
    /// sit in RAM at `0x80180000`/`0x80189000`/`0x80192000`, loaded by `func_800174D4`).
    pub vnao: [u8; 2],
    pub vm: Vm,
    /// Actors: animation command 1 ran (`D_8005F3D4`; cleared when a script starts).
    pub done: bool,
}

impl Default for SdStage {
    fn default() -> Self {
        SdStage {
            objects: [SdObject { layer: -1, ..Default::default() }; SLOTS],
            active: false,
            vnao: [0; 2],
            vm: Vm::Sd,
            done: false,
        }
    }
}

enum Ctl {
    Continue,
    Stop,
}

impl SdStage {
    /// `func_8002EE6C`: start script `id`. Unknown ids kill everything.
    pub fn start(&mut self, bank: &SdBank, id: u8) {
        self.start_records(bank, bank.script(id), SLOTS);
    }

    /// `func_8002EF44` (request `0x2xx`): start script `id` read as 20-byte records, whose
    /// extra `i16 x, y` set each object's start position (−1 → 0) with no velocity.
    pub fn start_placed(&mut self, bank: &SdBank, id: u8) {
        self.active = true;
        let mut rec = bank.script(id);
        for obj in self.objects.iter_mut() {
            let layer = rec.and_then(|r| bank.i32(r)).unwrap_or(0);
            if layer <= 0 {
                rec = None;
                obj.layer = -1;
                continue;
            }
            let r = rec.unwrap();
            let pos = |o: u32| match bank.u16(r + o).map(|v| v as i16) {
                Some(-1) | None => 0,
                Some(v) => v,
            };
            obj.layer = layer;
            obj.flip = bank.i32(r + 4).unwrap_or(0);
            obj.anim = bank.u32(r + 8).unwrap_or(0);
            obj.motion = bank.u32(r + 12).unwrap_or(0);
            obj.vx = 0;
            obj.vy = 0;
            obj.x = pos(16);
            obj.y = pos(18);
            obj.scale = [0x80; 2];
            obj.scale_count = 0;
            obj.anim_wait = 0;
            obj.wait = 0;
            rec = Some(r + 20);
        }
    }

    /// An actor stage (objects beyond [`ACTOR_SLOTS`] stay dead).
    pub fn actor() -> Self {
        SdStage { vm: Vm::Actor, ..Default::default() }
    }

    /// `func_80031AC8`: start an actor from its records at `rec` (see
    /// [`SdBank::actor_script`]).
    pub fn start_actor(&mut self, bank: &SdBank, rec: Option<u32>) {
        self.done = false;
        self.start_records(bank, rec, ACTOR_SLOTS);
    }

    fn start_records(&mut self, bank: &SdBank, mut rec: Option<u32>, slots: usize) {
        self.active = true;
        for (i, obj) in self.objects.iter_mut().enumerate() {
            if i >= slots {
                rec = None;
            }
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
        let mut fx = Effects { sounds: &mut sounds, vnao: &mut self.vnao, done: &mut self.done, vm: self.vm };
        for obj in &mut self.objects {
            obj.visible = obj.alive();
            if obj.visible {
                motion(obj, bank, fx.vm);
                anim(obj, bank, &mut fx);
            }
        }
        sounds
    }

    /// Objects to draw this frame, in slot order.
    pub fn visible(&self) -> impl Iterator<Item = &SdObject> {
        self.objects.iter().filter(|o| o.visible && o.frame.is_some())
    }
}

/// Where animation commands leave their side effects.
struct Effects<'a> {
    sounds: &'a mut Vec<u8>,
    vnao: &'a mut [u8; 2],
    done: &'a mut bool,
    vm: Vm,
}

/// `func_8002F094` (actors: `func_80032180`).
fn motion(o: &mut SdObject, bank: &SdBank, vm: Vm) {
    let was = o.wait;
    if was == 0 {
        let mut p = o.motion;
        for _ in 0..MAX_OPS {
            p = motion_op(o, bank, p, vm);
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

/// `func_8002F1A8` (actors: `func_80032294`): one motion op at `p`; returns the next op's
/// address.
fn motion_op(o: &mut SdObject, bank: &SdBank, p: u32, vm: Vm) -> u32 {
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
                // Both VMs call the actor's op 4 (`func_800324B4`) here. For an actor that is a
                // jump to (x, y), run on through the next op. SD_CHR's own pointer only moves
                // past n (the x word is read as the next op), and the helper instead moves the
                // actor VM's last object with the actor's script pointer. No script on the disc
                // (all 256 SD_CHR ids, every actor mood) has n = 0, so only the pointer step is
                // kept for SD_CHR.
                if vm == Vm::Actor {
                    o.vx = 0;
                    o.vy = 0;
                    o.x = arg(1) as i16;
                    o.y = arg(2) as i16;
                    return p + 16;
                }
                return p + 8;
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
fn anim(o: &mut SdObject, bank: &SdBank, fx: &mut Effects) {
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
            let (next, ctl) = command(o, bank, p, fx);
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
/// Actors use `func_8003269C` (`jtbl_80011F0C`), which differs in commands 1 and 7.
fn command(o: &mut SdObject, bank: &SdBank, p: u32, fx: &mut Effects) -> (u32, Ctl) {
    let arg = |i: u32| bank.i32(p + 8 + 4 * i).unwrap_or(0);
    let actor = fx.vm == Vm::Actor;
    match bank.u16(p + 4).unwrap_or(0xFFFF) {
        0 => (bank.u32(p + 8).unwrap_or(p), Ctl::Continue),
        1 if actor => {
            // func_800327A0: the pointer stays on this command, so it repeats every frame.
            *fx.done = true;
            (p, Ctl::Stop)
        }
        7 if actor => {
            // func_80032BCC: RAM slots 1–4 (0x80180000/0x80189000/0x80192000/0x8019B000):
            // 0–3 → slot 1–4 as page 26; 4 → slot 3, 5 → slot 4, 6 → slot 1 as page 27. Solo
            // and mode 7 hold VNAO1–3 there (slot 4 empty); vs CPU the stage's character sheets.
            match arg(0) {
                n @ 0..=3 => fx.vnao[0] = n as u8 + 1,
                4 => fx.vnao[1] = 3,
                5 => fx.vnao[1] = 4,
                6 => fx.vnao[1] = 1,
                _ => {}
            }
            (p + 12, Ctl::Continue)
        }
        4 if actor => {
            // func_8003280C: whole-number scale (+0x24/+0x26) and step (+0x32/+0x34),
            // step = (target − scale) / n truncated; the accumulator below mirrors that exactly.
            let n = arg(0) as i16;
            o.scale_count = n;
            for i in 0..2 {
                let target = arg(1 + i as u32) as u16 as i16;
                if n != 0 {
                    let step = target.wrapping_sub(o.scale[i]) / n;
                    o.scale_fx[i] = i32::from(o.scale[i]) << 12;
                    o.scale_step[i] = i32::from(step) << 12;
                } else {
                    o.scale[i] = target;
                    o.scale_fx[i] = i32::from(target) << 12;
                }
            }
            (p + 20, Ctl::Continue)
        }
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
            // func_8002F814: the targets are sign-extended halfwords.
            let target = [arg(1) as u16 as i16 as i32, arg(2) as u16 as i16 as i32];
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
            fx.sounds.push(arg(0) as u8);
            (p + 12, Ctl::Continue)
        }
        7 => {
            let vnao = &mut *fx.vnao;
            match arg(0) {
                0 => vnao[0] = 1,
                1 => vnao[0] = 2,
                2 => vnao[1] = 3,
                3 => vnao[0] = 3,
                _ => {}
            }
            (p + 12, Ctl::Continue)
        }
        // 3 and 6 loop forever in the original.
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

/// Draw mode 4 (`func_80030070` → `func_8001CBE4`): a quad scaled about the object's position
/// itself (its frame offset scales too), `(x0, y0, w, h, mirrored)` in screen pixels. The
/// function subtracts the screen centre from its corners, which the ordering table's offset
/// adds back.
pub fn panel_placement(o: &SdObject, f: &Frame) -> (i32, i32, i32, i32, bool) {
    let [sx, sy] = o.scale.map(i32::from);
    let (x, y) = (i32::from(o.x), i32::from(o.y));
    let x0 = x + ((i32::from(f.dx) * sx) >> 7);
    let y0 = y + ((i32::from(f.dy) * sy) >> 7);
    let dw = (i32::from(f.w) * (sx + 2)) >> 7;
    let dh = (i32::from(f.h) * (sy + 2)) >> 7;
    (x0, y0, dw, dh, o.flip != i32::from(f.flip))
}

/// Where `func_80032934` draws an actor object: `(x, y, scale)` to hand to the sprite draw
/// `func_8001B148` (which adds the frame's offset). Positions are relative to the screen
/// centre; a scaled object is pulled back so that it grows about its frame's centre.
pub fn actor_placement(o: &SdObject, f: &Frame) -> (i32, i32, i32) {
    let s = i32::from(o.scale[0]);
    let (x, y) = (i32::from(o.x), i32::from(o.y));
    if s == 0x80 || f.w == 0 || f.h == 0 {
        return (x + 0xA0, y + 0x78, s);
    }
    let half = (s - 0x80) / 2 * 100;
    let dx = half / (0x3200 / i32::from(f.w)).max(1);
    let dy = half / (0x3200 / i32::from(f.h)).max(1);
    (x - (dx - 0xA0), y - (dy - 0x78), s)
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

    /// A move over n = 0 frames: an actor jumps to (x, y) and goes on to the op after it; SD_CHR
    /// only steps past n, so it reads the x word as its next op.
    #[test]
    fn zero_frame_move_differs_by_vm() {
        let mut d = vec![0u8; 0x40];
        let mut w = |at: usize, v: u32| d[at..at + 4].copy_from_slice(&v.to_le_bytes());
        w(0x00, 1); // op 1, n = 0, x = 2, y = 7
        w(0x04, 0);
        w(0x08, 2);
        w(0x0C, 7);
        w(0x10, 2); // op 2: wait 5
        w(0x14, 5);
        let b = SdBank { data: d };
        let start = SdObject { x: 40, y: 40, vx: 3, vy: 3, motion: BASE, ..Default::default() };

        let mut actor = start;
        motion(&mut actor, &b, Vm::Actor);
        assert_eq!((actor.x, actor.y, actor.vx, actor.vy), (2, 7, 0, 0));
        assert_eq!((actor.motion, actor.wait), (BASE + 0x18, 4));

        let mut sd = start;
        motion(&mut sd, &b, Vm::Sd);
        // x word = op 2 with the y word (7) as its wait; the position only moves by velocity.
        assert_eq!((sd.motion, sd.wait), (BASE + 0x10, 6));
        assert_eq!((sd.vx, sd.vy), (0, 0));
    }

    #[test]
    fn scale_grows_from_the_bottom_centre() {
        let o = SdObject { x: 100, y: 64, scale: [64, 64], ..Default::default() };
        let f = Frame { w: 120, h: 64, ..Default::default() };
        // Half size, centred on x 160 with the bottom at 128; the quad is (s + 2)/128 wide.
        assert_eq!(placement(&o, &f), (130, 96, 61, 33, false));
    }
}
