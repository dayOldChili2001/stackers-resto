//! Rule constants read from the executable. Each item cites its address; see `docs/gameplay.md`.

/// Piece gravity, indexed by a level's speed index (`0x80012104`).
/// High half = pixels per step, low half = frames between steps.
pub const GRAVITY: [u32; 22] = [
    0x8001E, 0x8001C, 0x8001A, 0x80018, 0x80016, 0x80014, 0x80012, 0x80010, 0x8000E, 0x8000C,
    0x8000B, 0x8000A, 0x80008, 0x80006, 0x80004, 0x80002, 0x80001, 0x90001, 0xA0001, 0xC0001,
    0xE0001, 0x100001,
];

/// The 16 words before [`GRAVITY`] (`0x800120C4`–`0x80012100`, other tables). Solo mode's
/// slow-downs (`func_80039B4C`) can push the speed index below 0, and the game then reads these.
const GRAVITY_BEFORE: [u32; 16] = [
    0x20, 0x536B, 0x446B, 0x486B, 0x1, 0x10020, 0x4, 0x30020, 0x4, 0x50020, 0x5, 0x70020,
    0x120005, 0x90020, 0x1E0005, 0xC0010,
];

/// `lw D_80012104[index]` for any index the game can reach (clamped below −16 and above 21).
pub fn gravity(index: i16) -> u32 {
    if index < 0 {
        GRAVITY_BEFORE[(16 + index).max(0) as usize]
    } else {
        GRAVITY[(index as usize).min(GRAVITY.len() - 1)]
    }
}

/// Solo mode (mode 4) plays level-table row 3 (`func_80039CEC`).
pub const SOLO_ROW: usize = 3;
/// Solo mode's starting speed cap `D_801005F6`, raised by one (up to 17) each time it is reached.
pub const SOLO_SPEED_CAP: i16 = 15;
/// CD-DA music index for solo mode (`func_80039EE4`, mode 4).
pub const SOLO_MUSIC: usize = 8;

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

/// CD-DA music for a level-table row in mode 13, as an index into the executable's music
/// table (`stackers_formats::music`; index *i* = disc track *i* + 2). `func_80039EE4` case
/// for mode 13 (`jtbl_80011FF4`): rows 0–2 → 5, rows 3–11 → 3, rows 12+ → 7, stored in
/// `D_800B1280` and started by `func_8003A050` → `func_80040AE8`.
pub const fn music_for_level(row: usize) -> usize {
    match row {
        0..=2 => 5,
        3..=11 => 3,
        _ => 7,
    }
}

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

/// Colour-0 (grey) and colour-6 script lengths from `BLT_ANM.OBJ`, for the states that end.
/// State 4's script is a bare done marker: done on its first step.
pub const ANIM_GREY: [(u8, u16); 4] = [(4, 0), (7, 47), (11, 20), (12, 45)];
pub const ANIM_SPECIAL: [(u8, u16); 5] = [(4, 0), (7, 48), (11, 23), (12, 47), (13, 48)];

/// How an arm-span script (`*(0x801C0020)`) runs, as stepped by `func_80024C14`: the step that
/// raises "done" and the `(step, n)` of each event (command 3), counting the first step as 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanTiming {
    pub done: u8,
    pub events: &'static [(u8, u8)],
}

const fn st(done: u8, events: &'static [(u8, u8)]) -> SpanTiming {
    SpanTiming { done, events }
}

/// Arm-span script table index (`func_80025234`): `mode` 1 reach, 0 cleared, −1 retract;
/// `end` −1/0/1 as stored at `+4`/`+5`; `len` empty cells spanned.
pub fn span_index(mode: i8, end: i8, len: u8) -> usize {
    let group = match (mode, end) {
        (1, 0) => 0,
        (1, _) => 6,
        (0, 0) => 11,
        (0, -1) => 17,
        (0, _) => 22,
        (-1, 0) => 27,
        (-1, -1) => 33,
        (-1, _) => 38,
        _ => 0,
    };
    group + usize::from(len)
}

/// `[left, right][span_index]`, read from `BLT_ANM.OBJ` (checked by `tests/real_disc.rs` in
/// stackers-formats). Groups of 6 (lengths 0–5) or 5 (0–4) entries, in [`span_index`] order.
pub const SPAN_SCRIPTS: [[SpanTiming; 43]; 2] = [
    // left (`*(0x801C0020) + 0`)
    [
        st(51, &[(7, 1)]),
        st(51, &[(7, 1)]),
        st(58, &[(4, 1), (9, 2)]),
        st(57, &[(4, 1), (9, 2), (11, 3)]),
        st(61, &[(4, 1), (8, 2), (9, 3), (10, 4)]),
        st(63, &[(4, 1), (9, 2), (10, 3), (11, 4), (12, 5)]),
        st(53, &[]),
        st(53, &[]),
        st(57, &[(5, 1)]),
        st(61, &[(5, 1)]),
        st(61, &[(4, 1), (6, 2)]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(51, &[]),
        st(51, &[]),
        st(12, &[(9, 1)]),
        st(12, &[(9, 1)]),
        st(12, &[(4, 2), (9, 1)]),
        st(12, &[(4, 3), (7, 2), (9, 1)]),
        st(12, &[(3, 4), (5, 3), (7, 2), (9, 1)]),
        st(12, &[(3, 5), (6, 4), (7, 3), (8, 2), (9, 1)]),
        st(12, &[]),
        st(12, &[]),
        st(12, &[(9, 1)]),
        st(12, &[(9, 1)]),
        st(12, &[(7, 2), (9, 1)]),
        st(13, &[]),
        st(13, &[]),
        st(13, &[]),
        st(19, &[]),
        st(13, &[]),
    ],
    // right (`*(0x801C0020) + 0xAC`)
    [
        st(60, &[(10, 1)]),
        st(60, &[(10, 1)]),
        st(58, &[(4, 1), (9, 2)]),
        st(57, &[(4, 1), (8, 2), (11, 3)]),
        st(61, &[(4, 1), (8, 2), (9, 3), (10, 4)]),
        st(63, &[(4, 1), (9, 2), (10, 3), (11, 4), (12, 5)]),
        st(53, &[(9, 1)]),
        st(53, &[(9, 1)]),
        st(57, &[(5, 1)]),
        st(61, &[(5, 1), (7, 2)]),
        st(61, &[(4, 1), (6, 2)]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(48, &[]),
        st(51, &[]),
        st(51, &[]),
        st(12, &[(9, 1)]),
        st(12, &[(9, 1)]),
        st(12, &[(4, 2), (10, 1)]),
        st(12, &[(4, 3), (7, 2), (9, 1)]),
        st(12, &[(3, 4), (5, 3), (7, 2), (9, 1)]),
        st(12, &[(3, 5), (5, 4), (6, 3), (7, 2), (9, 1)]),
        st(12, &[(9, 1)]),
        st(12, &[(9, 1)]),
        st(12, &[(9, 1)]),
        st(12, &[(5, 3), (7, 2), (9, 1)]),
        st(12, &[(7, 2), (9, 1)]),
        st(13, &[]),
        st(13, &[]),
        st(13, &[]),
        st(19, &[]),
        st(13, &[]),
    ],
];
