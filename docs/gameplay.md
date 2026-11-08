# Tecmo Stackers — gameplay rules (reverse-engineered)

Source: `SLUS_003.15` disassembly (`decomp/asm`, not committed). Every rule cites the
function or data address it came from. Addresses are RAM (`0x8001_0000` = file offset `0x800`).
"p" = player index; most globals are per-player arrays.

Status legend: **verified** = read directly from asm; **assumed** = not yet traced, chosen to be
plausible. `stackers-core` follows this document.

## Frame order (verified, mode loop `func_80015AAC` etc.)

Each 60 Hz frame, per player:

1. Pad read (`func_80014F20`): `held`, `pressed` (edge), `prev_held`.
2. Player update `func_8001E0B8`: player-state handler (jump table `0x800104C8`), then, if the
   player state is 0 or 3, the controls: tick counters `func_8001FA64`, `func_8001EB4C`
   (rotate → move → fall).
3. Cell update `func_800202E8`: for rows `H-2 … 0` (bottom up), columns `1 … 6`:
   apply pending state → state handler → if non-empty: fall step `func_80021804` + animation step
   `func_80021D44`. Then a histogram of cell states is built (`D_800662B8`, 14 counters per player)
   which the **next** frame's player update reads.

## Field (verified)

- `D_800B1A40[p][col 8][row 79]`, cell struct 0x38 bytes. Column stride 0x1148, row stride 0x38.
- Height `D_800FFD00` = 19 in the normal modes (`func_80016B18` etc.), 79 in mode 7.
- Init `func_8002277C`: everything empty; columns 0 and 7 and the last row (`H-1`) are walls
  (state 2, colour 0xFF). Play area is columns 1–6, rows 0–17.
- Pixel coordinates: cell (c, r) is at x = 16c, y = 16r.

Cell fields: `+0` state, `+2` colour (0 none, 1–5 blobs, 6 special, 0xFF wall), `+4` pending
state, `+8` fall speed (1/128 px), `+0xC` x px, `+0x10` y px, `+0x14` falling flag,
`+0x16` just-landed, `+0x1C..0x28` animation script/timer/done, `+0x2A` "excited" (−1),
`+0x2C` arms (bit0 = left, bit1 = right), `+0x2E` flood-fill visited.

## Cell states (verified, jump table `0x80010534`)

| state | meaning | handler | exit |
|---|---|---|---|
| 0 | empty | – | – |
| 1 | falling | – (physics in `func_80021804`) | lands → 2 |
| 2 | settled | `func_80020A24` | excited & player state 7 → 8 |
| 4 | clearing | `func_80020C58` | anim done → 0, cells above fall |
| 5 | clearing (had arms) | `func_80020C58` | as 4 |
| 8 | arms reaching | `func_800212B8` | anim done → 9 |
| 9 | arms holding | `func_80021398` | anim done → 10 |
| 10 | arms retracting | `func_8002146C` | anim done → 2, arms cleared |

States 3, 6, 7, 11, 12, 13 belong to special blocks / other modes and are not used by plain
single-player.

Pending state (`+4`, applied at the start of the cell's update, `func_80020604`):
1 → becomes falling (speed 0x80, keeps `excited` if it was set); 4 → becomes 4, or 5 if the cell
was in state 10.

### Falling physics (verified, `func_80021804`, `func_80033964`, `func_8002193C`)

- Speed starts at 0x80 (= 1 px/frame), +0x20 per frame.
- If `speed/128 ≥ 17`: probe `y+16`, then `y += speed/128`; else `y += speed/127`.
  After moving, collide if the cell at `(x/16, y/16 + 1)` is state 2 (or 13).
- Landing: the cell moves to `(x/16, y/16)` as state 2, `just_landed = 1`, `excited` carried over.
  The original slot is emptied. Falling cells do not collide with other falling cells.

### Animation lengths (verified from `BLT_ANM.OBJ`, loaded at `0x801C0000`)

Script lookup `func_80021E18`: `*(*(0x801C0004 + 4·colour) + 4·state)`, done marker
`(0xFFFF, 1)` handled by `func_80021D44`. For colours 1–5:

| state | frames until done |
|---|---|
| 4 / 5 clear | 47 |
| 8 reach | 12 |
| 9 hold | 30 |
| 10 retract | 13 |

The done flag is seen by the state handler one frame after the last frame elapses.

## Arms (verified)

The defining mechanic. When the player state is 7, every settled cell that is "excited"
(`+0x2A == −1`) extends an arm toward each side whose immediate neighbour is empty
(`func_80020A24`: right → bit1, left → bit0) and enters state 8. A cell becomes excited when an
orthogonal neighbour is cleared (`func_80020C58`, direction table `0x80057108`).

An arm conceptually reaches across empty cells along the row until the first non-empty cell.
In the flood fill (`func_80033DFC`) two same-colour blobs are connected if:

- they are orthogonally adjacent; or
- one has an arm toward the other along the row with only empty cells between; or
- one sits directly above or below the span of the other's arm.

Groups of **4 or more** are cleared (pending state 4).

## Matching (verified, `func_80033A14`)

- Seeds: non-empty coloured cells with arms or `just_landed`, and not visited. `just_landed` is
  reset after each seed.
- Each group ≥ 4 is cleared. If any group cleared: `chain += 1` (`D_800C3068`, reset at spawn),
  `cleared += total` (`P+0x28`), attack points via `func_8003AF24`:
  `(chainT[min(chain,9)] + groupT[min(groups,5)] + countT[min(total,11)] or 1) · total · 10`,
  added only when `chain ≥ 2`. Tables at `0x8001250C`, `0x8001252C`, `0x80012520`.

## Player (verified)

Struct `D_80100250 + 0x50·p`: `+0` x, `+4` y (px of the pivot), `+8` fall speed word,
`+0xC` state, `+0xE` pivot colour, `+0x10` satellite colour, `+0x12` rotation (0 up, 1 right,
2 down, 3 left; offsets table `0x800570E8`), `+0x14` rotate cooldown, `+0x16` flip cooldown,
`+0x18` move repeat delay, `+0x1A` move animation, `+0x1C` fall delay, `+0x1E` lock counter,
`+0x20` landings this piece.

Spawn (`func_8001FFD8`, `func_80022D6C`): x = 48 (column 3), y = 88, rotation 0, counters cleared,
colours from the queue.

### Player states (`func_8001E148`, table `0x800104C8`)

| state | handler | behaviour |
|---|---|---|
| 0 | `func_8001E270` | piece under control (controls run) |
| 1 | `func_8001E2A8` | after lock: wait no falling cells → match? 5 : 4 |
| 2 | `func_8001E35C` | intro countdown 0x8F frames → 4 |
| 3 | `func_8001E3D0` | landed: lock delay (controls still run) |
| 4 | `func_8001E588` | → 11 |
| 5 | `func_8001E5B8` | wait clears done → (any state-10 cells) ? 9 : 6 |
| 6 | `func_8001E694` | wait no falling; if `recheck`: recheck=0, match? 5 : 4; else → 7 |
| 7 | `func_8001E77C` | (excited cells reach) wait no state-8 → 8 |
| 8 | `func_8001E7CC` | wait no state-9; match? 5 : (recheck=1, 9) |
| 9 | `func_8001E888` | wait no state 10/11/7 → 6 |
| 10 | – | game over |
| 11 | `func_8001E910` | wait no falling; if cell (3,6) is settled → 10, else spawn → 0 |

### Controls (verified; PS1 bits as read by `func_80014F20`)

- **Rotate** (`func_8001EBF0`), only when the rotate cooldown is 0: ✕ (0x40) = rotation − 1,
  ○ (0x20) = rotation + 1. The target satellite cell is
  `((x+dx)/16, (y+dy+8)/16)`. If blocked, try the opposite side: move the pivot one cell away and
  still rotate. If both are blocked, flip (`func_8001EF2C`, flip cooldown 0): rotation 0 → 2 with
  y −= 16, otherwise → 0 with y += 16. Cooldowns: rotate ±8, flip ±5 frames.
- **Move** (`func_8001EF9C`), when the repeat delay is 0: right (0x2000) then left (0x8000) are
  each checked. Delay = 6 on first press, 2 while held from the previous frame. Blocked test
  `func_80033670`: cells `(x/16 ± 1, (y+8)/16)` for pivot and satellite.
- **Fall** (`func_8001F194`, state 0): soft drop if ↓ (0x4000) held and the move animation is 0
  (`func_8001F360`, speed 0x80001 unless the level is faster), else gravity when the fall delay is 0
  (`func_8001F41C`). Speed word: high half = px per step, low half = frames between steps
  (`func_8001F4D4`, steps > 8 px are split into 8 px probes).
- **Blocked below** (`func_80033508`): cell `(x/16, (y+16)/16)` for pivot and satellite is not
  empty (any state, including falling). Landing snaps y to a multiple of 16 and enters state 3.
- **Lock delay** (state 3): if the rotate cooldown is active, lock immediately once landings ≥ 9,
  else wait. Otherwise the counter += 3 with ↓ held, else += 1; lock at ≥ 25. While in state 3,
  if the piece can fall again it returns to state 0.
- **Lock** (`func_8001F67C`): pivot and satellite become falling cells (they split).

## Piece generation (verified, `func_8002319C`, `func_80023474`)

Queue of 3 pairs per player. Each colour = `rand() % colours + 1`, where `rand` is the PS1 BIOS
`A(0x2F)` LCG (`seed = seed·0x41C64E6D + 0x3039; return (seed >> 16) & 0x7FFF`). The satellite
colour is drawn before the pivot colour. In VS with equal colour counts the two players share a
32-entry ring buffer (`func_80022F58`).

## Levels and speed (verified)

Gravity table `0x80012104` (speed words, index 0–21): `0x8001E, 0x8001C, …, 0x80001, 0x90001,
0xA0001, 0xC0001, 0xE0001, 0x100001`.

Level tables: 8 tables × 14 entries at `0x8001218C` (stride 0x70), 8 bytes per entry:
`u16 colours, u16 variant, u16 quota, u16 speed index`. Applied by `func_80039CEC` (called from
the mode inits): colours → `D_800C3534/6`, variant → `D_800B1398/13A0` (selects a 12-byte record
per level from `D_8005588C`, copied to `D_800B13E8/13F4`; use not traced), quota → `P+0x2C`,
speed index → `P+0x2A`, gravity word → `P+8`.

Table and row selection (`func_80039CEC`):

- Mode 13 (`D_80100684`): table pointer `*(0x8001215C + 16·D_800C30C8)`, row `D_800662F0`.
  `D_800C30C8` 0, 1, 2 → tables `0x8001218C`, `0x8001218C`, `0x8001234C` (variant 1).
  A menu at `0x80029510` sets (`D_800C30C8`, `D_800662F0`) to (0, 0), (1, 3) or (2, 3); another
  path (`0x8002D524`) sets the row from `D_800C33F8`. Tables `0x800121FC`, `0x8001226C`,
  `0x800122DC`, `0x800123BC`, `0x8001242C`, `0x8001249C` are not selected here.
- Any other mode: row 3 of the first table (5 colours, quota 40, speed 4) and `D_801005F6 = 15`.

The first table:

| row | colours | quota | speed idx |
|---|---|---|---|
| 0–2 | 4 | 48 | 2, 5, 7 |
| 3–8 | 5 | 40 | 4–9 |
| 9–13 | 5 | 32 | 10–14 |

**Speed-up** (`func_80039A78` → `func_80039AC4`, called at the end of spawn `func_8001FFD8`,
every mode except 4 and 7): if `P+0x28` (blobs cleared, added in `func_80033A14`) `> P+0x2C`
(quota), subtract the quota from `P+0x28`; then, if the speed index is below 21, increment it and
load the new gravity word. At most one step per spawn. The colour count never changes during a
game; there is no level advance inside a game.

Mode 4 uses `func_80039B4C` instead: threshold 20 blobs, the speed index rises toward a cap
`D_801005F6`; on reaching it the cap rises (up to 17) and the index drops by 6 (not ported).

## Sprites and animation scripts (verified)

Implemented in `stackers-formats::anm` (parser + script player) and `stackers-web::sprites`.

- Sheets: `DERO/DERO0–9.TIM`, 4 bpp 256×256, all sharing a 16×7 CLUT at VRAM (0,480). Frame `page`
  n is `DEROn.TIM` (file table `0x80057298`, ids `0x47+n`, loaded by `func_80017744`); `clut`
  is the palette row (colour *c* uses row *c*−1).
- `BLT_ANM.OBJ` @ `0x801C0000`: word 0 → piece table `[colour][player_state]`
  (`func_8001F9F8`); from `+4`, one inline pointer per colour → cell table `[state]`
  (`func_80021E18`). For colours other than 0/6, states 2 and 4 index a further table by the
  neighbour mask; states 5, 8, 9, 10 by mask then by arm bits (jump table `0x8001056C`).
- Script: `(u32 duration, u32 frame)` pairs; duration `0xFFFF` = command (`1` done, `0` jump to
  next word, other = ignored). Player `func_80021D44`: `+0x1C` script, `+0x20` timer, `+0x22`
  frames in entry, `+0x24` frame, `+0x28` done. Timer `0xFF` freezes.
- Frame (18 bytes): `u, v, w, h, dx, dy, page, clut, flip`; drawn by `func_8001B148` as a
  POLY_FT4 at the cell position + (dx, dy), mirrored when `flip` ≠ the caller's flag (0).
- All reachable mask/arm variants of states 4/5/8/9/10 have the lengths in the Animation table
  (checked by `tests/real_disc.rs`).

### Neighbour mask `+0x34` (`func_80022590`, `func_800220A0`)

Bits: 8 up, 4 right, 2 left, 1 down; set when the neighbour has the same colour and state ≥ 2.
Recomputed at the end of the cell update for cells flagged dirty (`+0x32 = −1`), rows `H-2…1`,
skipping colours 0, 6 and walls. Dirty is set by landing (the cell, left, right, below) and,
when a clear makes a cell above start falling, on that cell's settled row neighbours. A changed
mask sets `+0x30 = 1`, so the next anim step restarts the script and replays the frames spent in
the current entry (`func_80021ED8`).

### Controlled pair

Player `+0x30` / `+0x40` are 16-byte script players for pivot / satellite (`func_8001F95C`), set
from the piece table at spawn (player state 0) and on landing (state 3), stepped by
`func_8001F928` every frame the player state is 0 or 3.

**Not yet ported:** the intro camera pan and zoom (camera state `D_801095B0`, `func_8001AA10`;
during play the camera is −16 and the scale 0x80, see "Screen layout"), the HUD numbers, menus.

## Screen layout (solo, verified)

Implemented in `stackers-formats::hud` and `stackers-web::screen`; `examples/screen.rs` renders it.

- Files load by index into the path table `D_80057298` (`func_8002CE68(index, dest)`; disc
  positions cached in `D_80012544` by `func_8002CFC0`). TIMs go to VRAM through `func_80018770`,
  which stores the tpage in `D_80066278[page]` and the 16 CLUT ids in `D_80103250[page]`.
  Solo pages: DERO0–9 = 0–9, PSFONT = 12, COMMON = 13, LEVEL2 = 17, stage B = 18, VS_WIN2 = 20,
  MSG2 = 22, W_ASA = 24 (`func_80017744`, `func_800183C4`, `func_80018464`).
- Background: `func_80018464` loads ST22A (or ST23A when `D_800C2FB8` ≠ 0) into RAM `0x80138000`,
  and `func_8001A504` copies it into the frame buffer each frame (`func_80015664`). It is
  posterized art with off-colour outline pixels, and the decode is correct as-is. The stage's 8-bit
  B copy (page 18) is ×(128/96) brighter.
- Sprite draw `func_8001B148(frame, x, y, flip, scale, layer)`: top-left `(x + dx·s/128,
  y + dy·s/128)`, size `(w + (s ≠ 128))·s/128`. The draw offset is the screen centre (the code
  subtracts 160/120). `func_8001C7A8` is the same plus a CLUT override, brightness and semi-transparency
  (blend mode 0); `func_8001B860` adds brightness. Lower OT layer = in front; within a layer the
  primitive added first is drawn last.
- Field (`func_800195FC`): screen = centre + (cell − centre)·`D_800FFDB8`/128 with x = cell x + 8
  (+0xB8 for player 2) and y = cell y − camera `D_800667A8` − 80. During play the camera is −16
  and the scale 0x80, so screen = (x + 8, y − 64). The intro pan starts the camera at 0x3B0.
- Frame `0x80056820` (W_ASA 192×240) drawn at (0, 0) on layer 1 by `func_8001A7BC`. Its
  see-through middle is exactly columns 1–6 and rows 6–17. It also contains the next-pair
  window (sarcophagus, x 140–171, y 20–136).
- Queue (`func_80023294(slot)`): x = `D_800B0F90` (0x93 in solo, `func_80017480`), slot y
  `D_800699F4` rests at 0x28/0x41/0x5A (`func_80023474`) and on a shift moves 1 px/frame toward
  `D_8010AD88` = 0/0x28/0x41 (`func_80022B94`). Scale = max(128 − y, 0x5B). The satellite is drawn at
  (x + (16 − s/8)/2, y + 0x2C), the pivot s/8 lower. Slot 0 also draws `0x80055F14` (ST22B 64×30)
  at (0x80, 0x84) with brightness 0x60. That patch matches the background and hides slot 2. **The shift
  animation is not ported**: the port draws the queue at rest.
- HUD panels `func_8001A560(c0, c1, c2, l0, l1, l2, semi)`: per panel at (200, 16/88/160), label
  `D_8005666C[l]` then the monitor `D_8005666C[0]` (96×72, semi-transparent), both with palette c.
  Solo call: palettes 1, 2, 4; labels 1 "Erase Number", 2 "Record", 6 "Course Time".
  Records 7–9 are Continue/Quit/Retry and 10 is "Falling Blocks".
- Numbers: `func_8001D4D8(value, digits, x, y, …)` (score via `func_8001E0B8`: mode 13 at
  (0x80, 0xD0)). Big 16×32 digits are PSFONT records `0x80056388 + 18·d`. Not ported yet.

## Sound effects (verified)

Implemented in `stackers-formats::{vab, sound}`, `stackers-core::se` and `stackers-web::sfx`.

- `func_80040740(id, kind)` dispatches on kind (jump table `0x80013AC4`); kind 1 =
  `func_800407C4`, a one-shot effect: entry `D_80013160[id]` = (bank slot, program, key, –),
  played with `SsVoKeyOn((vab << 8) | program, key << 8, D_80100388, D_80100358)`.
- Bank slots `D_80012A94` (0x44 bytes): 8-byte name + `u32` kind (0 `VAB_SE`, 1 `VAB_CHR`,
  2 XA). Slots 0–23 are the 24 VABs on the disc; slot 0 = `DEROMAIN`. Set up by `func_80040394`.
- VABs are standard v6 (`pBAV`); every tone covers a single key, so an effect is one sample.
  15 table entries (102, 160, 170–182, all `OUTGAME`) name keys their bank doesn't define.

In-game effects (all `DEROMAIN`):

| id | when | source |
|---|---|---|
| 2 | rotate, kick or flip attempt (also when the flip is on cooldown) | `func_8001EBF0` |
| 7 | sideways move that succeeded | `func_8001EF9C` |
| 3 | pair touches down (→ player state 3) | `func_8001F5EC` |
| 3 | a coloured blob landed this frame (flag `D_8010067C`, once per frame) | `func_8002193C`, `func_80021C68` |
| 100 | colour-0 cell landed, while `D_800B001C < 17` (not ported) | same |
| 9 + n − 1, max 17 | clear number *n* of a chain (counter `D_800B1380`) | `func_8002C770` |

## Music (verified)

Implemented in `stackers-formats::music`, `stackers_core::tables::music_for_level` and
`stackers-web::music`; `web/play.html` plays the raw track `.bin` through a WAV header.

- CD state machine `func_8003F98C`, state `D_800B11E0` (jump table `jtbl_800135E4`, index
  state + 2). States 0–0xD stream **XA** (setmode 0x48, setfilter file/channel, ReadS) from
  `D_80100654` → `0x80013468` (12-byte entries: file `SND_XA0n`, channel, `i32` sectors,
  `i32` flag), started by `func_80040A7C`. States 0xFE+ play **CD-DA** (setmode 7, setloc
  from TOC positions `D_800C2ED0`, play, then GetlocP until the end) from `D_8010062C` →
  `0x800134E0`, started by `func_80040AE8(index, kind)`.
- CD-DA table: 11 entries, `u8 track`, 3 pad, `i32 sectors`, `i32 flag`. Entry *i* = disc track
  *i* + 2; `sectors` = the track length after its 150-sector pregap, minus 1. Flag −1 (only
  track 2) = play once; 0 = seek back to the start and play again. Track 13 is not in the table.
- In-game choice (`func_80039EE4`, mode − 4 into `jtbl_80011FF4`, stored in `D_800B1280` and
  started by `func_8003A050` once `D_800C34B4` is set): mode 4 → 8, 7 → 10, 11 → 9,
  13 → by level row `D_800662F0` (0–2 → 5, 3–11 → 3, ≥ 12 → 7), 14 → 6, 15 → 4. Modes 5, 6,
  8, 9, 10 and 12 start no music here. `func_8003A050` also accepts an XA request
  (`D_800C34B2` ≠ 0, 3).
- Other CD-DA starts: index 1 (track 3) from `func_80015960` (if not already playing) and from
  `func_8002D628` when mode = 7 (kind 2); `func_80027300` (two sites) picks from `D_800C2EC0`.

## Open questions

- Which menu mode maps to which mode id (`D_80100684`; dispatcher `func_80014634`).
- Which mode is 13; what `D_800C33F8` and the per-level `D_8005588C` records are.
- Scoring display, special blocks (colour 6), garbage, CPU AI.
- What plays on title/menus/game over (`func_80027300`, XA table users).
- Intro countdown and game-over presentation timings beyond the state machine.
