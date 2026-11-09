//! One player's game: the player state machine (`func_8001E148`) and controls.

use crate::camera::{Camera, LockRecord};
use crate::field::{st, Field, Histogram, Matches, TALL_ROWS, TILE};
use crate::garbage::{blobs_for, Garbage};
use crate::over::{GameOver, Outcome};
use crate::pause::Pause;
use crate::mascot::{Cues, Mascot};
use crate::rng::BiosRand;
use crate::tables::{
    gravity,
    Level, ATTACK_CHAIN, ATTACK_COUNT, ATTACK_GROUPS, DIR, INTRO_FRAMES, LEVELS, SOFT_DROP, SOLO_ROW, SOLO_SPEED_CAP,
};

/// Pad bits as assembled by `func_80014F20` (active high).
pub mod pad {
    pub const L2: u16 = 0x0001;
    pub const R2: u16 = 0x0002;
    pub const L1: u16 = 0x0004;
    pub const R1: u16 = 0x0008;
    pub const TRIANGLE: u16 = 0x0010;
    pub const CIRCLE: u16 = 0x0020;
    pub const CROSS: u16 = 0x0040;
    pub const SQUARE: u16 = 0x0080;
    pub const SELECT: u16 = 0x0100;
    pub const START: u16 = 0x0800;
    pub const UP: u16 = 0x1000;
    pub const RIGHT: u16 = 0x2000;
    pub const DOWN: u16 = 0x4000;
    pub const LEFT: u16 = 0x8000;
}

/// Player states (jump table `0x800104C8`).
pub mod ps {
    pub const CONTROL: u8 = 0;
    pub const SETTLE: u8 = 1;
    pub const INTRO: u8 = 2;
    pub const LANDED: u8 = 3;
    pub const NO_MATCH: u8 = 4;
    pub const CLEARING: u8 = 5;
    pub const AFTER_CLEAR: u8 = 6;
    pub const REACHING: u8 = 7;
    pub const HOLDING: u8 = 8;
    pub const RETRACTING: u8 = 9;
    pub const GAME_OVER: u8 = 10;
    pub const SPAWN: u8 = 11;
}

const SPAWN_X: i32 = 0x30;
const SPAWN_Y: i32 = 0x58;
/// The game ends when this cell is occupied at spawn time (`D_800B4F68`).
pub const DEATH_CELL: (usize, usize) = (3, 6);
/// Next-pair window slot y at game start and for a newly rolled pair (`func_80023474`,
/// `func_80022D6C`).
pub const QUEUE_START_Y: [i32; 3] = [0x28, 0x41, 0x5A];
/// Where each slot comes to rest (`D_8010AD88`); `func_80022B94` moves 1 px per frame up to it.
pub const QUEUE_REST_Y: [i32; 3] = [0, 0x28, 0x41];

/// Sound-effect ids as passed to `func_80040740(id, 1)`. The page resolves them through the
/// executable's table at `0x80013160` (bank, program, key).
pub mod se {
    /// Rotate or flip (`func_8001EBF0`).
    pub const ROTATE: u8 = 2;
    /// Pair touches down (`func_8001F5EC`) or a blob lands (`func_80021C68`).
    pub const LAND: u8 = 3;
    /// Arms start reaching (`D_800FFDBC`, once per frame, `func_800248F4`).
    pub const REACH: u8 = 4;
    /// Sideways move (`func_8001EF9C`).
    pub const MOVE: u8 = 7;
    /// An arm pokes a grey blob (`func_8002602C`).
    pub const POKE: u8 = 8;
    /// Clear number `n` of a chain plays `CHAIN + n - 1`, capped at 17 (`func_8002C770`).
    pub const CHAIN: u8 = 9;
}

/// Which mode's rules a game follows (`D_80100684`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rules {
    /// Mode 13's speed-up (`func_80039AC4`).
    #[default]
    Standard,
    /// Mode 4, "Insane": `func_80039B4C`, the erase-number record, the pause menu.
    Solo,
    /// Mode 7: a 79-row field, fixed gravity, no speed-up, □ drops the pair as colour 0,
    /// the camera, and the longest chain as the record.
    ChainReaction,
    /// Mode 11, Time Trial (`func_80016BB4`): two players, both on level row 3 with the
    /// quota speed-up (`func_80039AC4`), a 0xB4-frame intro, all garbage colour 6, and the
    /// clock stops at 59:59.59. Player 1 is a [`Game::target`].
    TimeTrial,
}

/// `func_8001FCA0`: the intro countdown `D_80068510[p]` in mode 11.
const TIME_TRIAL_INTRO: i16 = 0xB4;
/// `func_8001E910`/`func_8002DA70`: Time Trial ends when player 1 holds this many settled blobs.
pub const TARGET_SETTLED: u16 = 0x48;

/// Chain Reaction's gravity and soft-drop speed words (`func_8001FCA0`: `P+8`, `D_800C3038`).
const CHAIN_REACTION_GRAVITY: u32 = 0x80010;
const CHAIN_REACTION_SOFT_DROP: u32 = 0x140001;

/// A pair in the queue: (pivot colour, satellite colour).
pub type Pair = (u8, u8);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub x: i32,
    pub y: i32,
    speed: u32,
    pub state: u8,
    pub pivot: u8,
    pub satellite: u8,
    pub rot: u8,
    rot_cd: i16,
    flip_cd: i16,
    move_delay: i16,
    move_anim: i16,
    fall_delay: i16,
    lock: i16,
    landings: i16,
    /// `D_80100618`: how far (px << 12) the drawn pair still trails `y` after a gravity step.
    fall_offset: i32,
    /// `D_80100620`: what [`Player::fall_offset`] loses per gravity frame.
    fall_rate: i32,
}

/// Where `func_80019998` draws the controlled pair, in field pixels (screen = x + 8,
/// y − camera − 80, before the zoom). `panel_satellite` is the satellite's offset from the pivot
/// in Chain Reaction's "Falling Blocks" panel, where the pivot sits at (0xF0, 0xC0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairDraw {
    pub pivot: (i32, i32),
    pub satellite: (i32, i32),
    pub panel_satellite: (i32, i32),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Chain count of the current turn (`D_800C3068`); reset at spawn.
    pub chain: u16,
    pub max_chain: u16,
    /// Blobs cleared in total.
    pub cleared: u32,
    /// Blobs cleared since the last speed-up (`P+0x28`; the quota is subtracted at spawn).
    pub toward_speedup: u32,
    /// Attack points generated by chains (`func_8003AF24`).
    pub attack: u32,
    pub pieces: u32,
}

/// What the solo HUD shows (mode 4, `func_80017098`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hud {
    /// `D_80069658` (`func_8003A82C`).
    pub score: u32,
    /// "Erase Number" `D_8010040C`: blobs whose clear finished, capped at 99,999,999.
    pub erased: u32,
    /// "Course Time" `D_80100648`, one byte each (low to high): frames, seconds, minutes, hours.
    pub time: u32,
}

impl Hud {
    pub const MAX_ERASED: u32 = 99_999_999;

    /// (hours, minutes, seconds) as `func_8003DCB8` unpacks them.
    pub fn hms(&self) -> (u32, u32, u32) {
        ((self.time >> 24) & 0x3F, (self.time >> 16) & 0x3F, (self.time >> 8) & 0x3F)
    }
}

/// `func_8003D8D8`: one frame of the packed clock; wraps to 0 after 23:59:59. In mode 11
/// (`capped`) a minute overflow stops it at 59:59 and 59 frames (`0x3B3B3B`).
pub(crate) fn tick_clock(t: u32, capped: bool) -> u32 {
    let t = t + 1;
    if t & 0xFF < 60 {
        return t;
    }
    let t = (t & !0xFF) + 0x100;
    if t & 0xFF00 < 0x3C00 {
        return t;
    }
    let t = (t & 0xFFFF_0000) + 0x1_0000;
    if capped && t & 0xFF_0000 > 0x3B_FFFF {
        return 0x3B_3B3B;
    }
    if t & 0xFF_0000 <= 0x3B_FFFF {
        return t;
    }
    let t = (t & 0xFF00_0000) + 0x100_0000;
    if t & 0xFF00_0000 > 0x17FF_FFFF {
        0
    } else {
        t
    }
}

/// The pair's sprite script (`P+0x30`/`P+0x40`, `func_8001F9F8`): chosen by the player state when
/// it was last restarted (at spawn and on landing), then stepped every controlled frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PieceAnim {
    pub id: u32,
    pub state: u8,
    pub age: u16,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub piece_anim: PieceAnim,
    pub field: Field,
    pub player: Player,
    pub queue: [Pair; 3],
    /// Slot y in the next-pair window (`D_800699F4`, stride 0x30).
    pub queue_y: [i32; 3],
    /// Frames the queue's idle scripts have run since the last shift (`func_80022BF8`; they
    /// restart on every shift, whose count is [`Stats::pieces`]).
    pub queue_age: u16,
    pub stats: Stats,
    pub hud: Hud,
    /// Attack points not yet scored (`D_80066206`, added by `func_8003AF24`).
    pending_attack: u16,
    /// Soft-drop frames × 2 (`D_80069524`); every 16 is worth 100 points.
    drop_points: u16,
    /// `D_800667A4`: set when the intro ends, cleared at game over; the clock runs while set.
    clock_running: bool,
    /// Starting row of the level table (`D_800662F0`); fixed for the whole game.
    pub level: usize,
    /// Current gravity index (`P+0x2A`), raised by `func_80039AC4`.
    pub speed_index: i16,
    pub rules: Rules,
    /// Solo speed cap `D_801005F6`.
    speed_cap: i16,
    /// `D_80066AD0`: frames since the game started (`func_8003D830`).
    play_frames: u32,
    /// `D_800C2FE4`: 10-minute slow-downs already applied.
    slowdowns: i32,
    held: u16,
    colors: u8,
    pub(crate) rng: BiosRand,
    hist: Histogram,
    intro: i16,
    recheck: bool,
    pub(crate) prev_held: u16,
    pub frame: u64,
    /// Sound effects triggered during the last [`Game::step`], in order, without duplicates.
    pub sounds: Vec<u8>,
    /// The best erase number so far (`D_800B0F3C`), set by the caller; game over raises it.
    pub record: u32,
    /// The sequence after game over (mode 4 sub-state 4).
    pub over: GameOver,
    /// The pause menu (sub-state 3); solo and Chain Reaction.
    pub pause: Pause,
    /// Chain Reaction's camera and zoom.
    pub camera: Camera,
    /// Slots of the last locked pair (`D_800C3000`), for the camera.
    pub lock_record: LockRecord,
    /// Length of the chain in progress (`D_800B1380`, `func_8002C770`); 0 between chains.
    pub chain_len: u32,
    /// The chain value seen last frame (`D_80066798`).
    chain_seen: u16,
    /// Chain Reaction's mascot and her voice lines (`None` in other modes).
    pub mascot: Option<Mascot>,
    /// Garbage sent to this player (two-player modes).
    pub garbage: Garbage,
    /// Blobs that dropped into the field this frame.
    pub garbage_dropped: u16,
    /// Column 1–6 whose garbage is colour 6 (`D_800C3132 + 1`), set by the two-player HUD.
    pub special_column: Option<usize>,
    /// The opponent's player state when this player's cells update (garbage commits on 11/0).
    pub(crate) opponent_state: u8,
    /// Blobs this frame's score step sends to the opponent (`func_8003A82C`).
    pub(crate) outgoing: u16,
    erased_before: u32,
    was_over: bool,
    /// Two-player piece ring `D_80100318` (swapped in by [`crate::Versus`]) and this player's
    /// read head `D_800B0054[p]`.
    pub(crate) ring: PieceRing,
    pub(crate) ring_read: u16,
    pub(crate) shared_ring: bool,
    /// Time Trial's player 1: no controls (`func_8001E148` alone, so not even gravity), and
    /// state 11 (`func_8001E910`) never spawns: it goes back to 4 to take more garbage, to 10
    /// once [`Game::settled`] reaches 0x48 with the spawn cell filled, or to 0 (stuck) when
    /// that cell fills sooner.
    pub target: bool,
}

/// `D_80100318[32]` and its write head `D_8006626E`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PieceRing {
    colors: [u8; 32],
    write: u16,
}

impl PieceRing {
    fn push(&mut self, c: u8) {
        self.colors[usize::from(self.write & 31)] = c;
        self.write = self.write.wrapping_add(1);
    }

    fn get(&self, i: u16) -> u8 {
        self.colors[usize::from(i & 31)]
    }
}

impl Game {
    pub fn new(seed: u32, level: usize) -> Self {
        let level = level.min(LEVELS.len() - 1);
        let lv = LEVELS[level];
        let mut g = Game {
            piece_anim: PieceAnim::default(),
            field: Field::new(),
            player: Player {
                x: SPAWN_X,
                y: SPAWN_Y,
                speed: gravity(i16::from(lv.speed)),
                state: ps::INTRO,
                pivot: 0,
                satellite: 0,
                rot: 0,
                rot_cd: 0,
                flip_cd: 0,
                move_delay: 0,
                move_anim: 0,
                fall_delay: 0,
                lock: 0,
                landings: 0,
                fall_offset: 0,
                fall_rate: 0,
            },
            queue: [(0, 0); 3],
            queue_y: QUEUE_START_Y,
            queue_age: 0,
            stats: Stats::default(),
            hud: Hud::default(),
            pending_attack: 0,
            drop_points: 0,
            clock_running: false,
            level,
            speed_index: i16::from(lv.speed),
            rules: Rules::Standard,
            speed_cap: SOLO_SPEED_CAP,
            play_frames: 0,
            slowdowns: 0,
            held: 0,
            colors: lv.colors,
            rng: BiosRand::new(seed),
            hist: Histogram::default(),
            intro: INTRO_FRAMES,
            recheck: false,
            prev_held: 0,
            frame: 0,
            sounds: Vec::new(),
            record: 0,
            over: GameOver::default(),
            pause: Pause::default(),
            camera: Camera::default(),
            lock_record: LockRecord::default(),
            chain_len: 0,
            chain_seen: 0,
            mascot: None,
            garbage: Garbage::default(),
            garbage_dropped: 0,
            special_column: None,
            opponent_state: ps::INTRO,
            outgoing: 0,
            erased_before: 0,
            was_over: false,
            ring: PieceRing::default(),
            ring_read: 0,
            shared_ring: false,
            target: false,
        };
        // func_80023474: the satellite colour is drawn first.
        for i in 0..3 {
            let sat = g.roll();
            let piv = g.roll();
            g.queue[i] = (piv, sat);
        }
        g
    }

    /// Solo mode (mode 4): level row 3 and the solo speed-up rules.
    pub fn solo(seed: u32) -> Self {
        Game { rules: Rules::Solo, ..Game::new(seed, SOLO_ROW) }
    }

    /// Chain Reaction (mode 7, `func_80015AAC` sub-state 0): level row 3's colours, then
    /// `func_8001FCA0` fixes the gravity; the field is 79 rows tall. `record` is the longest
    /// chain so far (`D_80067224`).
    pub fn chain_reaction(seed: u32, record: u32) -> Self {
        let mut g = Game { rules: Rules::ChainReaction, record, ..Game::new(seed, SOLO_ROW) };
        g.field = Field::with_rows(TALL_ROWS);
        g.player.speed = CHAIN_REACTION_GRAVITY;
        g.mascot = Some(Mascot::new(g.play_frames));
        g
    }

    /// `D_800667A4`: the clock runs (set when the intro ends, cleared at the end).
    pub fn clock_running(&self) -> bool {
        self.clock_running
    }

    pub(crate) fn stop_clock(&mut self) {
        self.clock_running = false;
    }

    /// Restart the intro countdown for the current rules.
    pub(crate) fn reset_intro(&mut self) {
        self.intro = self.intro_frames();
    }

    /// `func_8001FCA0`'s intro countdown.
    fn intro_frames(&self) -> i16 {
        if self.rules == Rules::TimeTrial {
            TIME_TRIAL_INTRO
        } else {
            INTRO_FRAMES
        }
    }

    /// Settled blobs in the play area as of the last cell update (the histogram
    /// `D_800662B8[p][2]`, walls excluded).
    pub fn settled(&self) -> u16 {
        self.hist.get(st::SETTLED)
    }

    /// Solo or Chain Reaction: the modes with a pause menu and a game-over sequence.
    fn single_player(&self) -> bool {
        matches!(self.rules, Rules::Solo | Rules::ChainReaction)
    }

    pub fn level_info(&self) -> Level {
        LEVELS[self.level]
    }

    /// How the game was left: from the game-over countdown or the pause menu.
    pub fn outcome(&self) -> Option<Outcome> {
        self.over.outcome.or(self.pause.outcome)
    }

    pub fn is_over(&self) -> bool {
        self.player.state == ps::GAME_OVER
    }

    /// True while the player controls a falling pair.
    pub fn piece_active(&self) -> bool {
        matches!(self.player.state, ps::CONTROL | ps::LANDED)
    }

    /// `D_80066AD0`: frames of play so far (drives the off-screen arrows' animation).
    pub fn play_frames(&self) -> u32 {
        self.play_frames
    }

    /// `func_80019998`'s positions for the pair: a sideways move draws the pivot 8 px back
    /// toward where it came from while the move animation runs, a flip nudges both halves,
    /// gravity steps are spread over their frames (`D_80100618`), and a rotation eases the
    /// satellite round the pivot over the rotate cooldown (`P+0x14` × 128, 4096 = 360°).
    pub fn pair_draw(&self) -> PairDraw {
        let p = &self.player;
        let x = p.x - 8 * i32::from(p.move_anim.signum());
        let y = p.y - (p.fall_offset >> 12);
        let flip = i32::from(p.flip_cd.signum());
        let (dx, dy) = DIR[p.rot as usize];
        let (dx, dy) = rotate_z(i32::from(p.rot_cd) << 7, dx, dy);
        PairDraw {
            pivot: (x, y + 8 * flip),
            satellite: (x + dx, y - 10 * flip + dy),
            panel_satellite: (dx, -0x12 * flip + dy),
        }
    }

    /// Satellite position in pixels.
    pub fn satellite_xy(&self) -> (i32, i32) {
        let (dx, dy) = DIR[self.player.rot as usize];
        (self.player.x + dx, self.player.y + dy)
    }

    fn sound(&mut self, id: u8) {
        if !self.sounds.contains(&id) {
            self.sounds.push(id);
        }
    }

    fn roll(&mut self) -> u8 {
        (self.rng.next() % u32::from(self.colors) + 1) as u8
    }

    /// Advance one 60 Hz frame with the pad state `held`.
    pub fn step(&mut self, held: u16) {
        let pressed = held & !self.prev_held;
        self.sounds.clear();
        if self.over.active {
            // Mode 4 sub-state 4 draws the field and the queue but updates neither.
            let mut sounds = Vec::new();
            if self.over.step(pressed, |id| sounds.push(id)) {
                self.field.burst_step();
            }
            self.field.step_span_segments();
            sounds.into_iter().for_each(|id| self.sound(id));
            self.step_mascot();
            self.prev_held = held;
            self.frame += 1;
            return;
        }
        if self.pause.active {
            // Sub-state 3: only the menu runs (and, in mode 7, the camera).
            self.step_camera(held, pressed, true);
            let mut sounds = Vec::new();
            self.pause.step(pressed, |id| sounds.push(id));
            sounds.into_iter().for_each(|id| self.sound(id));
            self.field.step_span_segments();
            self.step_mascot();
            self.prev_held = held;
            self.frame += 1;
            return;
        }
        self.play_player(held, pressed);
        self.play_cells(held, pressed);
        if self.is_over() {
            self.end();
        } else if self.single_player() && self.clock_running && pressed == pad::START {
            // func_80017098 / func_80015AAC: Start (alone, `D_800B1908 == 0x800`) while the
            // clock runs.
            self.pause.begin();
        }
        self.prev_held = held;
        self.frame += 1;
    }

    /// The first half of a frame of play: the clock and the player update `func_8001E0B8`
    /// (state handler, then the controls). Two-player modes run both players' halves before
    /// either cell update.
    pub(crate) fn play_player(&mut self, held: u16, pressed: u16) {
        self.sounds.clear();
        self.held = held;
        self.play_frames += 1;
        // func_8003D830 runs before the mode handler.
        if self.clock_running {
            self.hud.time = tick_clock(self.hud.time, self.rules == Rules::TimeTrial);
        }
        self.erased_before = self.field.erased;
        self.was_over = self.is_over();
        self.player_state(held);
        if self.piece_active() && !self.target {
            self.controls(held, pressed);
            self.piece_anim.age = self.piece_anim.age.saturating_add(1);
        }
    }

    /// The second half: the cell update `func_800202E8` (garbage, queue, cells), the spans,
    /// the score and everything after it.
    pub(crate) fn play_cells(&mut self, held: u16, pressed: u16) {
        // func_800239A0, then func_80022A5C, at the start of the cell update.
        self.garbage_dropped = self.garbage.step(
            &mut self.field,
            &mut self.rng,
            self.player.state,
            self.opponent_state,
            self.special_column,
            self.rules == Rules::TimeTrial,
        );
        if !self.was_over {
            self.step_queue();
        }
        self.hist = self.field.update(self.player.state);
        if std::mem::take(&mut self.field.blob_landed) {
            self.sound(se::LAND);
        }
        // func_800248C4: the arm spans, after the cells (the poke sound comes first).
        let (reach, poke) = self.field.step_spans(self.player.state);
        if poke {
            self.sound(se::POKE);
        }
        if reach {
            self.sound(se::REACH);
        }
        self.score();
        self.step_camera(held, pressed, false);
        // func_800258A8 runs with the field draw.
        self.field.step_span_segments();
        self.track_chain();
        // func_80031730, after the chain counter and before the end check (func_8002DC58).
        self.step_mascot();
        // func_8002CBC0 / func_8002CBE8: the drop in the field's blob counts this frame.
        let erased = self.field.erased - self.erased_before;
        self.hud.erased = (self.hud.erased + erased).min(Hud::MAX_ERASED);
        if self.is_over() {
            self.clock_running = false;
        }
    }

    /// `func_8002DBAC` (mode 4) on the frame the player reaches state 10: the record, then the
    /// burst. Mode 7's `func_8002DC58` never shows a new record (`D_800B1238 = 1`).
    fn end(&mut self) {
        let new_record = match self.rules {
            Rules::ChainReaction => false,
            _ => {
                let r = self.hud.erased >= self.record;
                if r {
                    self.record = self.hud.erased;
                }
                r
            }
        };
        self.over.begin(new_record);
        let rng = &mut self.rng;
        self.field.burst(|| rng.next());
    }

    fn player_state(&mut self, held: u16) {
        let h = self.hist;
        match self.player.state {
            ps::SETTLE => {
                if h.get(st::FALLING) == 0 && h.get(st::SOLID_13) == 0 {
                    self.player.state = if self.check() { ps::CLEARING } else { ps::NO_MATCH };
                }
            }
            ps::INTRO => {
                if self.intro == 0 {
                    self.intro = self.intro_frames();
                    self.clock_running = true;
                    self.player.state = ps::NO_MATCH;
                } else {
                    self.intro -= 1;
                }
            }
            ps::LANDED => {
                if self.player.rot_cd != 0 {
                    if self.player.landings >= 9 {
                        self.lock_piece();
                    }
                } else {
                    self.player.lock += if held & pad::DOWN != 0 { 3 } else { 1 };
                    if self.player.lock >= 25 {
                        self.lock_piece();
                    }
                }
            }
            ps::NO_MATCH => self.player.state = ps::SPAWN,
            ps::CLEARING => {
                if [st::CLEARING, st::CLEARING_ARMS, st::CLEAR_SPECIAL, st::SPECIAL_12]
                    .iter()
                    .all(|&s| h.get(s) == 0)
                {
                    self.player.state = if h.get(st::RETRACT) == 0 { ps::AFTER_CLEAR } else { ps::RETRACTING };
                }
            }
            ps::AFTER_CLEAR => {
                if h.get(st::FALLING) == 0 {
                    if self.recheck {
                        self.recheck = false;
                        self.player.state = if self.check() { ps::CLEARING } else { ps::NO_MATCH };
                    } else {
                        self.player.state = ps::REACHING;
                    }
                }
            }
            ps::REACHING => {
                if h.get(st::REACH) == 0 {
                    self.player.state = ps::HOLDING;
                }
            }
            ps::HOLDING => {
                if h.get(st::HOLD) == 0 {
                    if self.check() {
                        self.player.state = ps::CLEARING;
                    } else {
                        self.player.state = ps::RETRACTING;
                        self.recheck = true;
                    }
                }
            }
            ps::RETRACTING => {
                if h.get(st::RETRACT) == 0 && h.get(st::SPECIAL_11) == 0 && h.get(st::CLEAR_SPECIAL) == 0 {
                    self.player.state = ps::AFTER_CLEAR;
                }
            }
            ps::SPAWN => {
                if h.get(st::FALLING) == 0 && h.get(st::SOLID_13) == 0 {
                    let (c, r) = DEATH_CELL;
                    let full = self.field.cell(c, r).state == st::SETTLED;
                    if self.target {
                        self.player.state = match (full, h.get(st::SETTLED) >= TARGET_SETTLED) {
                            (false, _) => ps::NO_MATCH,
                            (true, true) => ps::GAME_OVER,
                            (true, false) => ps::CONTROL,
                        };
                    } else if full {
                        self.player.state = ps::GAME_OVER;
                    } else {
                        self.spawn();
                    }
                }
            }
            _ => {}
        }
    }

    /// Match check plus the bookkeeping done in `func_80033A14`.
    fn check(&mut self) -> bool {
        let m: Matches = self.field.check_matches();
        if m.group_sizes.is_empty() {
            return false;
        }
        let s = &mut self.stats;
        s.chain += 1;
        s.max_chain = s.max_chain.max(s.chain);
        let chain_sound = (u16::from(se::CHAIN) + s.chain - 1).min(17) as u8;
        let total = m.total();
        s.cleared += total;
        s.toward_speedup += total;
        // func_8003AF24
        let chain = usize::from(s.chain.min(9));
        let groups = m.group_sizes.len().min(5);
        let count = (total as usize).min(11);
        let mut power = ATTACK_CHAIN[chain]
            .wrapping_add(ATTACK_GROUPS[groups])
            .wrapping_add(ATTACK_COUNT[count]);
        if power == 0 {
            power = 1;
        }
        // The product is truncated to 16 bits before the × 10, and uses the clamped count.
        let points = (i32::from(power.wrapping_mul(count as u16) as i16) * 10) as u16;
        if s.chain >= 2 {
            s.attack += u32::from(points);
            self.pending_attack = self.pending_attack.wrapping_add(points);
        }
        self.sound(chain_sound);
        true
    }

    /// `func_8003A82C`: pending attack points are worth 100 each and send points / divisor + 1
    /// blobs to the opponent; otherwise every 16 soft-drop units (8 frames of ↓) are worth 100.
    fn score(&mut self) {
        if self.pending_attack != 0 {
            let sent = blobs_for(self.pending_attack, self.play_frames, false);
            let points = i32::from(self.pending_attack as i16) * 100;
            self.hud.score = self.hud.score.wrapping_add(points as u32);
            self.pending_attack = 0;
            if sent != 0 {
                self.outgoing = self.outgoing.wrapping_add(sent);
                self.garbage.sent_this_turn = self.garbage.sent_this_turn.wrapping_add(sent);
            }
        } else if self.drop_points as i16 >= 16 {
            self.drop_points -= 16;
            self.hud.score = self.hud.score.wrapping_add(100);
        }
    }

    /// `func_80039AC4` (called at the end of spawn): once more than `quota` blobs have been
    /// cleared, subtract the quota and raise gravity one step, up to index 21. At most one step
    /// per spawn; the colour count never changes.
    fn speed_up(&mut self) {
        match self.rules {
            Rules::Solo => return self.solo_speed_up(),
            Rules::ChainReaction => return,
            Rules::Standard | Rules::TimeTrial => {}
        }
        let quota = u32::from(LEVELS[self.level].quota);
        if self.stats.toward_speedup > quota {
            self.stats.toward_speedup -= quota;
            if self.speed_index < 21 {
                self.speed_index += 1;
                self.player.speed = gravity(self.speed_index);
            }
        }
    }

    /// `func_80039B4C` (solo): every 20 blobs past 20 raise the speed index toward the cap; on
    /// reaching it the cap rises (up to 17) and the index drops by 6. Every 10 minutes of play
    /// the index drops by 4, and holding L1 at spawn drops it by 1 while it is 8 or more.
    fn solo_speed_up(&mut self) {
        let mut idx = self.speed_index;
        if self.stats.toward_speedup >= 21 {
            self.stats.toward_speedup -= 20;
            if idx < self.speed_cap {
                idx += 1;
            } else {
                if self.speed_cap <= 16 {
                    self.speed_cap += 1;
                }
                idx -= 6;
            }
            self.set_speed(idx);
        }
        if self.slowdowns < (self.play_frames / 36_000) as i32 {
            self.slowdowns += 1;
            self.set_speed(self.speed_index - 4);
        }
        if self.speed_index >= 8 && self.held & pad::L1 != 0 {
            self.set_speed(self.speed_index - 1);
        }
    }

    fn set_speed(&mut self, idx: i16) {
        self.speed_index = idx;
        self.player.speed = gravity(idx);
    }

    /// `func_8001FFD8` + `func_80022D6C`.
    fn spawn(&mut self) {
        let p = &mut self.player;
        p.x = SPAWN_X;
        p.y = SPAWN_Y;
        p.rot = 0;
        p.rot_cd = 0;
        p.landings = 0;
        p.lock = 0;
        p.move_anim = 0;
        p.fall_delay = 0;
        p.state = ps::CONTROL;
        self.restart_piece_anim();
        self.stats.chain = 0;
        self.stats.pieces += 1;
        let (piv, sat) = self.queue[0];
        self.player.pivot = piv;
        self.player.satellite = sat;
        self.queue[0] = self.queue[1];
        self.queue[1] = self.queue[2];
        self.queue[2] = self.next_pair();
        // The slots keep their y, so the pairs slide up; the new one enters at the bottom.
        self.queue_y = [self.queue_y[1], self.queue_y[2], QUEUE_START_Y[2]];
        self.queue_age = 0;
        self.speed_up();
    }

    /// `func_80022F14`: the next pair for queue slot 2. With a shared ring (two players with the
    /// same colour count, `func_80022F58`) a player that has read everything rolls two colours
    /// into it (satellite first) and both players read the same sequence; otherwise
    /// `func_8002319C` rolls them directly. Alone, both give the same colours.
    pub(crate) fn next_pair(&mut self) -> Pair {
        if !self.shared_ring {
            let sat = self.roll();
            let piv = self.roll();
            return (piv, sat);
        }
        if self.ring_read >= self.ring.write {
            let a = self.roll();
            self.ring.push(a);
            let b = self.roll();
            self.ring.push(b);
        }
        let sat = self.ring.get(self.ring_read);
        let piv = self.ring.get(self.ring_read.wrapping_add(1));
        self.ring_read = self.ring_read.wrapping_add(2);
        (piv, sat)
    }

    /// Mode 7's `func_8001B034` and `func_8001AA10`.
    fn step_camera(&mut self, held: u16, pressed: u16, paused: bool) {
        if self.rules != Rules::ChainReaction {
            return;
        }
        self.camera.zoom(held);
        let field = &self.field;
        self.camera.step(held, pressed, self.player.state, paused, &self.lock_record, |(c, r)| field.cell(c, r).y);
        // func_800195FC → func_80019998 (also while paused) moves the idle camera with the pair.
        if self.piece_active() {
            self.camera.follow_pair(self.pair_draw().pivot.1);
        }
    }

    /// `func_80031730`, once per mode 7 frame in every sub-state.
    fn step_mascot(&mut self) {
        let Some(mut mascot) = self.mascot.take() else { return };
        let cues = Cues {
            chain: self.stats.chain,
            play_frames: self.play_frames,
            paused: self.pause.active,
            over: self.over.active,
            record: self.record,
        };
        let rng = &mut self.rng;
        if let Some(line) = mascot.step(cues, || rng.next()) {
            self.sound(line);
        }
        self.mascot = Some(mascot);
    }

    /// `func_8002C770`, every play frame: each change of the chain value to a non-zero one
    /// lengthens the chain; when it drops to 0 the length is compared with the record (mode 7's
    /// longest chain `D_80067224`) and reset.
    fn track_chain(&mut self) {
        let chain = self.stats.chain;
        let prev = std::mem::replace(&mut self.chain_seen, chain);
        if chain == prev {
            return;
        }
        if chain != 0 {
            self.chain_len += 1;
        } else {
            if self.rules == Rules::ChainReaction && self.chain_len >= self.record {
                self.record = self.chain_len;
            }
            self.chain_len = 0;
        }
    }

    /// `func_80022A5C`: each slot moves 1 px up toward its rest position, then its scripts step.
    fn step_queue(&mut self) {
        for (y, rest) in self.queue_y.iter_mut().zip(QUEUE_REST_Y) {
            if rest < *y {
                *y -= 1;
            }
        }
        self.queue_age = self.queue_age.saturating_add(1);
    }

    /// `func_8001F67C`: both halves become falling blobs.
    fn lock_piece(&mut self) {
        let p = &self.player;
        let (sx, sy) = self.satellite_xy();
        let (pc, pr) = ((p.x / TILE) as usize, (p.y / TILE) as usize);
        let (sc, sr) = ((sx / TILE) as usize, (sy / TILE) as usize);
        let (piv, sat) = (p.pivot, p.satellite);
        self.field.drop_blob(pc, pr, piv);
        self.field.drop_blob(sc, sr, sat);
        self.lock_record = LockRecord { pivot: (pc, pr), pivot_color: piv, satellite: (sc, sr) };
        let p = &mut self.player;
        p.state = ps::SETTLE;
        p.pivot = 0xFF;
        p.satellite = 0xFF;
    }

    fn occupied(&self, x: i32, y: i32) -> bool {
        self.field.at(x / TILE, y / TILE).state != st::EMPTY
    }

    fn controls(&mut self, held: u16, pressed: u16) {
        // func_8001FA64
        let p = &mut self.player;
        toward_zero(&mut p.rot_cd);
        toward_zero(&mut p.flip_cd);
        // func_8001EB4C
        toward_zero(&mut p.move_anim);
        if p.fall_delay > 0 {
            p.fall_delay -= 1;
        }
        if p.move_delay > 0 {
            p.move_delay -= 1;
        }
        self.rotate(pressed);
        self.shift(held);
        self.fall(held, pressed);
    }

    /// `func_800332F4`: would the satellite collide after turning by `d`?
    fn rotation_blocked(&self, d: i32) -> bool {
        let k = (i32::from(self.player.rot) + d).rem_euclid(4) as usize;
        let (dx, dy) = DIR[k];
        self.occupied(self.player.x + dx, self.player.y + dy + 8)
    }

    /// `func_8001EBF0`.
    fn rotate(&mut self, pressed: u16) {
        if self.player.rot_cd != 0 {
            return;
        }
        let (d, cd) = if pressed & pad::CROSS != 0 {
            (-1, 8)
        } else if pressed & pad::CIRCLE != 0 {
            (1, -8)
        } else {
            return;
        };
        let new_rot = ((i32::from(self.player.rot) + d).rem_euclid(4)) as u8;
        if !self.rotation_blocked(d) {
            self.player.rot = new_rot;
            self.player.rot_cd = cd;
        } else if !self.rotation_blocked(-d) {
            // Kick: step the pivot away from the obstacle.
            let k = (i32::from(self.player.rot) - d).rem_euclid(4) as usize;
            self.player.x += DIR[k].0;
            self.player.y += DIR[k].1;
            self.player.rot = new_rot;
            self.player.rot_cd = cd;
        } else {
            // func_8001EF2C: quick flip in a one-wide gap (the sound plays even on cooldown).
            let p = &mut self.player;
            if p.flip_cd == 0 {
                if p.rot == 0 {
                    p.flip_cd = -5;
                    p.y += DIR[0].1;
                    p.rot = 2;
                } else {
                    p.flip_cd = 5;
                    p.y += DIR[2].1;
                    p.rot = 0;
                }
            }
        }
        self.sound(se::ROTATE);
    }

    /// `func_80033670`.
    fn shift_blocked(&self, d: i32) -> bool {
        let p = &self.player;
        let (sx, sy) = self.satellite_xy();
        let a = self.field.at(p.x / TILE + d, (p.y + 8) / TILE);
        let b = self.field.at(sx / TILE + d, (sy + 8) / TILE);
        a.state != st::EMPTY || b.state != st::EMPTY
    }

    /// `func_8001EF9C`.
    fn shift(&mut self, held: u16) {
        if self.player.move_delay != 0 {
            return;
        }
        for (bit, d) in [(pad::RIGHT, 1), (pad::LEFT, -1)] {
            if held & bit == 0 {
                continue;
            }
            let delay = if self.prev_held & bit != 0 { 2 } else { 6 };
            self.player.move_delay = delay;
            if !self.shift_blocked(d) {
                self.player.x += 16 * d;
                self.player.move_anim = delay * d as i16;
                self.sound(se::MOVE);
            }
        }
    }

    /// `func_80033508`.
    fn blocked_below(&self) -> bool {
        let p = &self.player;
        let (sx, sy) = self.satellite_xy();
        self.occupied(p.x, p.y + 16) || self.occupied(sx, sy + 16)
    }

    /// `func_8001F194`.
    fn fall(&mut self, held: u16, pressed: u16) {
        match self.player.state {
            ps::CONTROL => {
                if held & pad::DOWN != 0 && self.player.move_anim == 0 {
                    // func_8001F360
                    if self.blocked_below() {
                        self.land();
                    } else {
                        let soft = match self.rules {
                            Rules::ChainReaction => CHAIN_REACTION_SOFT_DROP,
                            _ => SOFT_DROP,
                        };
                        let s = if self.player.speed >> 16 < 9 { soft } else { self.player.speed };
                        self.descend(s);
                    }
                    self.player.fall_offset = 0;
                    self.drop_points = self.drop_points.wrapping_add(2);
                } else {
                    // func_8001F41C: the drawn pair catches up every gravity frame.
                    if self.player.fall_delay == 0 {
                        if self.blocked_below() {
                            self.land();
                        } else {
                            self.descend(self.player.speed);
                        }
                    }
                    let p = &mut self.player;
                    p.fall_offset = (p.fall_offset - p.fall_rate).max(0);
                }
                // func_800200F0 (mode 7): □ locks the pair where it is, as colour-0 blobs.
                // Checked after the fall step even if that landed the pair.
                if self.rules == Rules::ChainReaction && pressed & pad::SQUARE != 0 && self.player.move_anim == 0 {
                    self.player.pivot = 0;
                    self.player.satellite = 0;
                    self.lock_piece();
                }
            }
            ps::LANDED => {
                if !self.blocked_below() {
                    self.player.state = ps::CONTROL;
                }
            }
            _ => {}
        }
    }

    /// `func_8001F4D4`.
    fn descend(&mut self, speed: u32) {
        self.player.fall_delay = (speed & 0xFFFF) as i16;
        let mut dy = (speed >> 16) as i32;
        while dy >= 9 {
            self.player.y += 8;
            dy -= 8;
            if self.blocked_below() {
                self.land();
                dy = 0;
            }
        }
        self.player.y += dy;
        let delay = i32::from(speed as u16 as i16);
        self.player.fall_offset = dy << 12;
        self.player.fall_rate = if delay != 0 { (dy << 12) / delay } else { 0 };
    }

    /// `func_8001F5EC`.
    fn land(&mut self) {
        let p = &mut self.player;
        p.state = ps::LANDED;
        p.landings += 1;
        p.y = p.y / TILE * TILE;
        p.fall_offset = 0;
        p.fall_rate = 0;
        self.restart_piece_anim();
        self.sound(se::LAND);
    }

    fn restart_piece_anim(&mut self) {
        self.piece_anim = PieceAnim {
            id: self.piece_anim.id.wrapping_add(1),
            state: self.player.state,
            age: 0,
        };
    }
}

fn toward_zero(v: &mut i16) {
    if *v > 0 {
        *v -= 1;
    } else if *v < 0 {
        *v += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::tick_clock;

    #[test]
    fn clock_carries_and_wraps_after_a_day() {
        assert_eq!(tick_clock(0x00_00_00_3B, false), 0x00_00_01_00);
        assert_eq!(tick_clock(0x00_00_3B_3B, false), 0x00_01_00_00);
        assert_eq!(tick_clock(0x00_3B_3B_3B, false), 0x01_00_00_00);
        assert_eq!(tick_clock(0x17_3B_3B_3B, false), 0);
    }
}

/// `RotMatrixZ(angle)` + `ApplyMatrix` on `(x, y, 0)` (4096 = 360°, GTE fixed point).
fn rotate_z(angle: i32, x: i32, y: i32) -> (i32, i32) {
    if angle == 0 {
        return (x, y);
    }
    let t = f64::from(angle) * std::f64::consts::TAU / 4096.0;
    let (c, s) = ((t.cos() * 4096.0).round() as i32, (t.sin() * 4096.0).round() as i32);
    ((c * x - s * y) >> 12, (s * x + c * y) >> 12)
}
