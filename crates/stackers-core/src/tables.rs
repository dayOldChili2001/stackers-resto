//! Rule constants read from the executable. Each item cites its address; see `docs/gameplay.md`.

/// Piece gravity, indexed by a level's speed index (`0x80012104`).
/// High half = pixels per step, low half = frames between steps.
pub const GRAVITY: [u32; 22] = [
    0x8001E, 0x8001C, 0x8001A, 0x80018, 0x80016, 0x80014, 0x80012, 0x80010, 0x8000E, 0x8000C,
    0x8000B, 0x8000A, 0x80008, 0x80006, 0x80004, 0x80002, 0x80001, 0x90001, 0xA0001, 0xC0001,
    0xE0001, 0x100001,
];

/// Soft-drop speed word (`func_8001FCA0`, `D_800C3038`).
pub const SOFT_DROP: u32 = 0x80001;

/// One row of a level table (`0x8001218C`, 8 bytes per entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level {
    pub colors: u8,
    /// Blobs to clear (strictly more than) for each gravity step (`func_80039AC4`).
    pub quota: u16,
    pub speed: u8,
}

const fn lv(colors: u8, quota: u16, speed: u8) -> Level {
    Level { colors, quota, speed }
}

/// First level table at `0x8001218C`.
pub const LEVELS: [Level; 14] = [
    lv(4, 48, 2),
    lv(4, 48, 5),
    lv(4, 48, 7),
    lv(5, 40, 4),
    lv(5, 40, 5),
    lv(5, 40, 6),
    lv(5, 40, 7),
    lv(5, 40, 8),
    lv(5, 40, 9),
    lv(5, 32, 10),
    lv(5, 32, 11),
    lv(5, 32, 12),
    lv(5, 32, 13),
    lv(5, 32, 14),
];

/// Attack-point tables used by `func_8003AF24`.
pub const ATTACK_CHAIN: [u16; 10] = [0, 0, 10, 10, 10, 11, 11, 11, 11, 12]; // 0x8001250C
pub const ATTACK_GROUPS: [u16; 6] = [0; 6]; // 0x8001252C
pub const ATTACK_COUNT: [u16; 12] = [0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0]; // 0x80012520 (overlaps the table above)

/// Animation lengths in frames for colours 1–5, from `BLT_ANM.OBJ` (see docs).
pub const ANIM_CLEAR: u16 = 47;
pub const ANIM_REACH: u16 = 12;
pub const ANIM_HOLD: u16 = 30;
pub const ANIM_RETRACT: u16 = 13;

/// Intro countdown before the first piece (`func_8001FCA0`).
pub const INTRO_FRAMES: i16 = 0x8F;

/// Satellite offsets by rotation (`0x800570E8`): up, right, down, left.
pub const DIR: [(i32, i32); 4] = [(0, -16), (16, 0), (0, 16), (-16, 0)];
