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

| 6 | poked by an arm (grey only) | – | pending 11 / 12 from `func_80026184` |
| 7 | clearing (grey under a cleared arm) | `func_80021030` | as 4 |
| 11 | poked, released in player state 9 | `func_800215A8` | anim done → 4 |
| 12 | poked, released in player state 5 | `func_8002166C` | anim done → 4 |

States 6, 7, 11 and 12 come from arm spans (see "Arm spans"); 3 and 13 belong to special
blocks / other modes and are not used by plain single-player.

Pending state (`+4`, applied at the start of the cell's update, `func_80020604`, table
`jtbl_800104FC`): 1 → becomes falling (speed 0x80, keeps `excited` if it was set); 4 → becomes 4,
or (`func_800208E8`) 5 if the cell was in state 10, starting a "cleared" span for each arm;
6, 7, 11, 12, 13 → becomes that state; others are ignored.

Grey (colour 0) and colour-6 script lengths: state 4 is a bare done marker (done on its first
step); grey 7 = 47, 11 = 20, 12 = 45 frames; colour 6: 7 = 48, 11 = 23, 12 = 47, 13 = 48.

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

### Arm spans (verified, `D_800C3538`)

Implemented in `stackers_core::field::spans` (timings in `tables::SPAN_SCRIPTS`, checked
against the disc by `stackers-web/tests/spans.rs`) and drawn by `stackers-web::sprites`.

The stretched arm is a separate object per cell, 0x1C0 bytes at
`D_800C3538 + col·0x8A40 + row·0x1C0 + p·0x2140` (the player stride is only 19 rows; mode 7
uses player 0 only): `+0` col, `+1` row, `+2` colour, `+3` mode (shared by both sides),
`+4`/`+5` end type left/right, `+6`/`+7` length left/right (0 = inactive), `+8`/`+0x18` span
script players (16 bytes: script, frame, timer, age, done), `+0x28`/`+0xF4` joint lists
(count, target, above bits, below bits, then 0x14-byte entries: player + column offset).

- **Spawn** `func_80024CE0(col, row, dir, mode)`: mode 1 when a blob reaches (`func_80020A24`,
  right side first), 0 when a blob in state 10 is cleared (`func_800208E8`), −1 on the first
  frame of state 10 (`func_8002146C`, flag `+0x18` set by `func_80021398`). Walk k = 1… along
  the row; a cell is **empty for the arm if state 0 or colour 0** (grey blobs are passed
  over). For each empty cell: bit k−1 of "below" / "above" if the cell at row ±1 has the
  blob's colour (mode −1 also marks it mask-dirty); mode 0 calls `func_80034A7C` on it.
  At the first non-empty cell (the far blob, distance len + 1): same colour → target = len
  (mode 0; mode 1 only if the far blob has no arms) or mark it mask-dirty (mode −1).
  End type: mode 1: −1 if the far blob (not a wall) has arms or is excited, else 0, and set
  `D_800FFDBC` (sound 4, played once in `func_800248F4`); mode 0: 0 for a wall or a blob
  without arms, −1 if it is in state 5 or pending 4, else 1; mode −1: 0 if settled, 1 if
  state 5 or pending 4, else −1. Script `*(0x801C0020) + 0xAC·right + group + 4·len`
  (`func_80025234`; groups 0 / 0x18 for mode 1 end 0 / ≠0, 0x2C / 0x44 / 0x58 for mode 0 end
  0 / −1 / 1, 0x6C / 0x84 / 0x98 for mode −1). Mode −1 steps its script once immediately.
- **Joints** `func_80025338` (not for mode −1): an end piece if target ≠ 0
  (`*(0x801C0030) + 0x320` left / `0x348` right, index len − 1, drawn at offset len), then one
  piece per below bit, then per above bit (base 0 below / 0x190 above, plus 0x78 right for
  end 0, or 0xF0 left / 0x140 right for end ≠ 0; index len·(len−1)/2 + k − 1, offset k).
  Entries are 8 bytes: the first word for mode 1, the second for mode 0.
- **Script player** `func_80024C14`: as the cell player, plus command 3 = event with one
  argument word, returned to the caller.
- **Step** `func_800248F4` (via `func_800248C4`, after the cell update, rows `H−2…0`, columns
  1–6, right then left): an active side whose done flag is set is retired (length 0), else
  stepped; an event n pokes cell (col ± n, row) (`func_8002602C`): if it is **settled and
  grey**, pending 6, sound 8, and it joins the poke list `D_800B0060` (count `D_800B1990`).
  If `D_80100248` = 30, `func_80026334` ORs mask bits: the far blob (when target ≠ 0) gets
  left/right, blobs below the span get "up", above get "down" (a cosmetic join; the next
  mask recompute undoes it). After the loop, `func_80026184`: in player state 9 every poked
  cell gets pending 11, in 5 pending 12 (list cleared); otherwise the list waits. Then
  `D_80100248` += 1 in player states 5, 7, 8, else 0. (`func_8002691C`, a variant that clears
  the masks instead, is never called.)
- **Grey burst** `func_80034A7C(cell)`: for the cell's 4 neighbours, settled grey → state 7
  (script restarted and stepped once); then neighbours whose colour is not 0, 0x80 or 0xFF
  are excited. So clearing a blob whose arms cross grey blobs also clears the grey around
  the arm and excites what borders it.
- **Draw** `func_800258A8` (play, pause and game-over frames, after the field draw): the arm
  sprite (`func_80025EB0`) at its cell with palette row colour − 1, OT layer 1; joints
  (`func_80025A74`, stepped here, so they also animate while paused) only while
  `D_80100248` ≥ 31 and the mode is not −1, each at (col ± offset, row). Same zoom and
  visibility as the blobs. Mode 4 adds the window frame first (spans draw under it), mode 7
  adds the window and counters after (spans draw over them).

## Matching (verified, `func_80033A14`)

- Seeds: non-empty coloured cells with arms or `just_landed`, and not visited. `just_landed` is
  reset after each seed.
- Each group ≥ 4 is cleared. If any group cleared: `chain += 1` (`D_800C3068`, reset at spawn),
  `cleared += total` (`P+0x28`), attack points via `func_8003AF24`:
  `(chainT[min(chain,9)] + groupT[min(groups,5)] + countT[min(total,11)] or 1) · min(total,11) · 10`
  (the product before × 10 is truncated to 16 bits), added only when `chain ≥ 2`, to the
  pending points `D_80066206` (player 2: `D_8006620A`). Tables at `0x8001250C`, `0x8001252C`, `0x80012520`.

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

Mode 4 uses `func_80039B4C` instead (ported as `Game::solo`, `solo_speed_up`), at the end of spawn:
1. if `P+0x28 ≥ 21`: subtract 20; if speed index < cap `D_801005F6` (15 at init) index += 1, else
   (cap += 1 if cap ≤ 16) and index −= 6;
2. if `D_800C2FE4 < D_80066AD0 / 36000` (play frames, ticked by `func_8003D830` while the mode
   sub-state is 1, intro included; magic 0x7482296B >> 46): `D_800C2FE4 += 1`, index −= 4
   (`D_800C2FE4` is zeroed while the frame count is < 60);
3. if index ≥ 8 and P1 holds L1 (`D_800C2F28 & 4`, held pad word from `func_80014F20`): index −= 1.
Each change reloads `P+8` from `0x80012104[index]`; the index can go negative and then reads the
words before the table (`tables::gravity`).

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

**Intro:** solo has no intro presentation besides the 0x8F-frame countdown (player state 2)
and the queue sliding into place. The camera pan from 0x3B0 belongs to mode 7 only: the camera
state machine `func_8001AA10` (state `D_801095B0`, jump table `0x800103D4`) is called only by
the mode-7 handler `func_80015AAC`. Mode 4 sets the camera to −16 and the scale to 0x80 once
(`func_80018A08`) and never changes them. The mode-4 init also sets `D_800C3514 = 0x100`, but
no mode-4 path runs the script VM (`func_8002EC34`) before game over overwrites it, so it shows
nothing. Camera states (mode 7): 0 hold at 0x3B0; 1 idle; 2 manual, R1/R2 pan ±6 px with △
jumping a screen (pad words `D_80100240`, `D_800C34F8`); 3 follow the stack top − 200 (never
lower than before), → 4; 3/4 → 5 when the player is back in state 0; 5 after 30 frames, snap
to −16 → 1. The camera is clamped to −16…0x3B0.

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
- Queue: slot struct `D_800699F0 + 0x30·slot` (`+0` satellite colour, `+4` y, `+8` satellite
  script player, `+0x18` pivot colour, `+0x20` pivot script player). Drawn by `func_80023294(slot)`
  at x = `D_800B0F90` (0x93 in solo, `func_80017480`). Scale = max(128 − y, 0x5B). The satellite is
  drawn at (x + (16 − s/8)/2, y + 0x2C), the pivot s/8 lower. Slot 0 also draws `0x80055F14` (ST22B
  64×30) at (0x80, 0x84) with brightness 0x60. That patch matches the background and hides a new
  pair while it rises into view.
- Queue motion (implemented: `Game::queue_y`): the slots start at y 0x28/0x41/0x5A
  (`func_80023474`). Every frame of play, `func_800202E8` → `func_80022A5C` moves each slot 1 px up
  toward `D_8010AD88` = 0/0x28/0x41 (`func_80022B94`), steps its two scripts (`func_80022BF8`)
  and draws it. The 0x8F-frame intro is longer than the 40 px slide, so play starts at rest: front
  pair at full scale, top of the sarcophagus. On spawn (`func_80022D6C`), slots 0–1 take colours
  **and y** from the slot below, and slot 2 gets a new pair at 0x5A. Then every slot's scripts restart
  (`func_80022C94`). After game over the mode handler draws the queue without stepping it.
- Queue scripts (implemented: `AnimBank::queue_script`, `Sprites::queue_frames`): BLT_ANM `+0x24`
  (`D_801C0024`) is a table `[colour − 1] → script`, an idle loop of squash/stretch/blink frames,
  with a different phase per colour.
- HUD panels `func_8001A560(c0, c1, c2, l0, l1, l2, semi)`: per panel at (200, 16/88/160), label
  `D_8005666C[l]` then the monitor `D_8005666C[0]` (96×72, semi-transparent), both with palette c.
  Solo call: palettes 1, 2, 4; labels 1 "Erase Number", 2 "Record", 6 "Course Time".
  Records 7–9 are Continue/Quit/Retry and 10 is "Falling Blocks".
- The solo screen is **mode 4** (`D_80100684`; dispatcher jump table `0x80010000` → `func_80017098`).
  Mode 13 (`func_80016480`) and others use the two-player frame `func_8001A82C`.
- HUD numbers: see "Solo HUD".

## Solo HUD (verified, mode 4)

Implemented in `stackers_core::Hud` and `stackers-web::screen`. All drawn on OT layer 0 (over the frame).

- `func_8001D4D8(value, digits, x, y, clut, font, zeros)`: digits from least significant into a
  buffer, then drawn left to right with `func_8001B4D0` (= `func_8001B148` with a palette override).
  The pen always advances by digit 0's width; leading zeros are skipped unless `zeros`, the last
  digit is always drawn. Returns the pen x. Fonts (PSFONT, page 12, digit *d* = record *d*):
  1 → `0x800564F0` 8×16, 2 → `0x800565B8` 8×8, 3 → `0x80056388` 16×32, other → `0x8005643C` 16×16.
- `func_8001D29C(str, x, y, clut, font)`: 'A'–'Y' are records of `0x80056844` (font ≠ 0) or
  `0x80055F3C`; a space advances by record 0's width; "k" + char picks a glyph via jump table
  `0x800103EC` ("kH" `0x80056374`, "kS" `0x80056350`, "kD" `0x80056362`, others listed there).
- **Score** `D_80069658` (`func_8003A82C`, after the cell update): if the pending attack points
  `D_80066206` ≠ 0, score += pending · 100 and pending = 0; otherwise, if `D_80069524` ≥ 16, it
  drops by 16 and score += 100. `D_80069524` += 2 on every soft-drop frame (`func_8001F360`, even
  when blocked). Drawn by `func_8001DF84`: 8 digits, font 1, palette 1, zeros, at (0x84, 0xD0)
  (mode 13: 0x80, mode 7: 0x86).
- **Erase Number** `D_8010040C`: the field counts `D_80100634` (colours 1–5) and `D_801005FA`
  (colours 0 and 6) go +2 on lock (`func_8001F67C`) and −1 when a clear animation finishes
  (`func_80020C58`). `func_8002CBC0` snapshots them before the player update, `func_8002CBE8` adds
  any drop after the cell update; capped at 99,999,999.
- **Record** `D_800B0F38[mode == 4]`: at game over (`func_8002DBAC`) set to the erase number if
  that is ≥ it. Zeroed only at boot (`func_80014C30`); the port keeps it in `localStorage`.
- Both counters (`func_8002CC90`): < 10,000 → 4 digits, font 3, palette 0, no zeros at (0xD8, y);
  ≤ 999,999 → 6 digits, font 1, palette 1, zeros at (0xE0, y + 8); ≤ 99,999,999 → 8 digits at
  (0xD8, y + 8). y = 0x28 (erase), 0x70 (record).
- **Course Time** `D_80100648`, bytes (low to high) frames, seconds, minutes, hours (wraps to 0
  after 23:59:59; `func_8003D8D8`). Ticked by `func_8003D830` before the mode handler while the
  mode sub-state `D_800C34AA` is 1 and `D_800667A4` is set (by the intro handler `func_8001E35C`
  when the countdown ends; cleared at game over). `func_8003DCB8` draws " " at (0xD0, 0xC4), hours
  (2 digits, no zeros), "kH", minutes, "kS", seconds, all font 1 palette 1.

## Game over (verified, mode 4)

Implemented in `stackers_core::over`, `Field::burst`/`burst_step`, `stackers_formats::sdchr`
and `stackers-web` (`Session` runs the scripts, `Screen::solo` draws them).

- Mode 4 sub-states (`D_800C34AA`, `jtbl_800101B8`): 0 init, 1 play, 3 pause (see "Pause"),
  4 game over. `func_8002DBAC` runs at the end of every play frame: when the player is
  in state 10 it sets the record (`D_800B0F3C = erase` if erase ≥ record, `D_800B1238 = 0`;
  else `D_800B1238 = 1`), clears the clock flag, sets sub-state 4 and calls `func_8003B2B8`.
- Burst (`func_8003B3DC`): rows 17 → 0, columns 1–6, every non-empty cell gets
  `+6 vx = rand % 8 − 4` then `+8 vy = −(rand % 40) − 30` (BIOS `rand`, the piece RNG).
  `func_8003B140` each frame: if `y < 16·19`, `x += vx`, `y += vy / 128` (toward zero),
  `vy += 0x20`; else the cell is reset empty (`func_80022908`). Sub-state 4 runs neither the player
  nor the cell update, so cell animations freeze; the queue is drawn but not stepped.
- Sequence `func_8002D628`, phase `D_800B1420` (`jtbl_80011DEC`), counter `D_800C3530`
  (+1 at the top of each frame), limit `D_800B1A28`:
  - 0: request script 0x13 (new record) or 0x14 (`func_8002D91C`), → 1. Returns early: no
    scripts or burst this frame.
  - 1: after 300 frames: SE 0x1C, script 0x18, → 3.
  - 3: every 120 frames and on a ✕ press: SE 0x1C, next script (0x18 … 0x21 = "Continue?"
    9 … 0). Start: SE 0x1E, → 4. Reaching 0x22 ("Game Over"): limit 200, XA 7 (not ported), → 5.
    Both the timer and ✕ on one frame add 0x101 twice and go through the 0x2xx start path;
    the port advances once.
  - 4: after 60 frames: mode 4 sub-state 0 (retry). 5: after 200 frames: mode 0 (title; the
    page restarts instead).
  - Phases ≥ 1 then call `func_8002EB38` (scripts) and `func_8003B03C` (burst step).
- Script requests `D_800C3514`: `0x1xx` starts script xx with 16-byte records
  (`func_8002EE6C`), `0x2xx` with 20-byte records that add a start position (`func_8002EF44`).
  `func_8002EC34` strips the high byte when it starts one.
- `SD_CHR.OBJ` (file index 1, RAM `0x801A8000`): script table and VM, see the module docs of
  `stackers_formats::sdchr`. 25 objects of 0x48 bytes at `D_8005ECA4`. `D_8005EC8C` picks the
  draw: mode 2 (`func_80030070`) = `func_8001C414(frame, x − ((w·sx/128 − w)/2), y − (h·sy/128 − h),
  flip, sx, sy, layer 0, frame clut, 0x80)`, i.e. scaled about the bottom centre; `func_8001C414`
  draws a quad `(w·(sx+2))>>7` wide. Mode 0 (`func_8002F950`) adds a shadow and is used elsewhere.
- Results: 0x13 = VS_WIN2 (page 20) "You Win" rising from y 288, 0x14 = "You Lose" (clut 1)
  dropping from −48 with a bounce; both sway afterwards. 0x18–0x21 = MSG2 (page 22) panel +
  digit at (160, 128). 0x22 = MSG2 "Game Over" 120×64 at (100, 64), zooming from 10/128 in 10 frames.
- VS_WIN2 is uploaded at VRAM x 550, so its texture page starts 152 texels left of the image:
  frame u/v are page-relative (`Tim::page_origin`).

## Pause (verified, mode 4)

Implemented in `stackers_core::pause` and `Screen::solo`.

- Entry (`func_80017098`, sub-state 1, after the play update, before `func_8002DBAC`): while the
  clock flag `D_800667A4` is set, if the pressed word `D_800B1908[D_800C2FB8]` is exactly 0x800
  (Start alone) or the pad's connection state changes (`D_800B1440`/`D_800B1462` vs
  `D_8006626C/D`, not ported), sub-state 3 and `func_8002DD80` (menu init). Game over in the
  same frame wins. Mode 13 has its own handler; modes other than 4, 7 and 11 draw only the plain
  "pause" sprite `D_80055F28` at (0x78, 0x48).
- Sub-state 3 draws the field (scale 128) and the queue (not stepped) and runs `func_8002DE14`;
  the clock, the cells and the player don't advance. Afterwards Start alone in any menu state
  resumes (sub-state 1, `D_800C3514 = 0`).
- Menu `func_8002E0F4`: timer `D_8006850C` (+1 first), state `D_80103248` (`jtbl_80011E3C`),
  cursor `D_800B1400` (0 Continue, 1 Quit, 2 Retry). Buttons compare the pressed word for
  equality, so two new presses on one frame do nothing.
  - 0: Start → resume. ↑ / ↓ move with wrap. Highlight: cursor item palette 1 and brightness
    0x80, others palette 0 and 0x3C. ✕: SE 0xA7, timer 0, the item gets palette 5, then
    Continue → 4, Quit → 3, Retry → 1.
  - 1 (Retry) / 2 (Quit): once the timer reaches 0x3D, `func_80040A7C(1, 2)` (CD state 0xC,
    music off), then sub-state 0 (new game, `D_80100688 = D_800662F0` skips the stage reload)
    or mode 0 (title).
  - 3 ("Quit Game? ✕Yes △No"): ✕ → 2, △ → 0, both timer 0.
  - 4: resume.
- Drawing (`func_8001B860(frame, x, y, flip, scale, layer, clut, brightness)`, opaque POLY_FT4,
  layer 0, before the HUD numbers so in front of them), records of `D_8005666C`:
  states 2/3: 16 (confirmation) at (120, 96), palette 1; otherwise 7/8/9 (Continue / Quit /
  Retry) at (120, 96/112/128) with the item palettes and brightness, and 20 ("✕ Confirm") at
  (226, 212). Then always 15 ("Paused") at (128, 80) and 0 (monitor) at (112, 80) with palette
  4, and 17/18/19 ("Press Start Button to Continue", MSG2) at (50/138/210, 180) with palette 6.

## Title and mode menu (verified)

Implemented in `stackers_core::front`, `stackers_formats::title`, `stackers-web` (`FrontEnd`,
`Screen::title`); `examples/title.rs` renders it, `examples/sddump.rs` traces SD_CHR scripts.

- Modes (`D_80100684`, dispatcher `func_80014634`, jump table `0x80010000`): 0
  `func_80015850` (4 blank frames → 1), 1 title (`func_800158FC`), 2 attract demo
  (`func_80018F24`), 4 solo, 5 Arcade (`func_8003A7DC` + `func_80015F28`), 7 Chain Reaction,
  8 mode menu (`func_80015960`), 9 hand-off into a game (`func_80015A24`), 11, 12 Time Trial,
  13, 14/15, 16 ending (`func_80030798`, STAFF*.TIM), 17 Option (`func_8003DDD8`).
- Pad: `D_800B1910 + 0x40·pad` holds one frame counter per button (16 words, bit order), +1 while
  held, 0 when released (`func_80014F20`). The title's "Start" test is the Start counter == 1
  (`D_800B193C` pad 1, `D_800B197C` pad 2).
- Title init `func_80027240` (mode 1 sub-state 0): loads (`func_800174D4`) NULL.TIM → RAM
  `0x80138000`, ROGO_FF → page 3, MSG2 → 22, MODE → 4, LEVEL → 5, KABEE2A → 6, KABEE2B → 7,
  TROGO → 8, PAPER then PSFONT → 12, OPTION → 14, VNAO1–3 → RAM `0x80180000/0x80189000/
  0x80192000`, SD_CHR, DOMI_BG → `0x80160000`. Background map set 0 (`func_8002430C`).
  SD palettes `D_80103658[slot]` = 2, 0, 0, 0, 0, 7, 0, 2, 2.
- Title `func_80027300`, state `D_800C2EC0` (`jtbl_800105E8`), timer `D_801003DC` (+1 first):
  - 0: TECMO logo `D_80056E84` by `func_8001C03C(frame, 0, −20, 0, sx 128, sy, …)`: centred
    (offset by the screen centre), `x0 = x − (w·(sx+7)>>7)/2`, size `w·s>>7`. sy = 2t for
    t < 0x40, 128 until 0xEC, then 128 − 2(t − 0xEC); at t = 0x12C → 1 and CD-DA index 0
    (Track 02, plays once). Start → 2 with script 0x103 and the same music.
  - 1: for t < 0x78 the 240×224 logo `D_80055C78` (page 3) is a GsSPRITE (`func_8001CF84`) at
    the screen centre, rotation `D_8010ADF8` += 30° per frame (4096 = 1°), scale `D_80066AD4`
    += t (4096 = 1). At t = 0x78: script 0x102 and → 2 in the same frame, the VM run with
    draw mode 0 (`func_8002EB08`). Start → 2 with 0x103.
  - 2: VM (`func_8002EAD4`, draw mode 1) and background each frame. Start → 3 with 0x104 (the
    title slides off right); t ≥ 0x960 → 4.
  - 3: after 0x14 frames, 0x105 and mode 8.
  - 4: brightness `D_801036A4` −2 per frame (SD objects only, no background); below 2 the
    music stops → 5; 5: after 4 frames, load the game art and mode 2 (attract, not ported: the
    port restarts the title).
- Scripts: 2 = logo drops onto KABEE2A/B (pages 6/7, the character art), 3 = same, at rest,
  4 = everything slides right, 5 = menu slides in, 6 = menu leaves, 8/0xB = Arcade/Time Trial
  screens, 0x5F = Option.
- SD draw modes (`D_8005EC8C`, `func_80030070`): 0 = palette `D_80103658[slot]`, brightness
  128; 1 = same with brightness `D_801036A4`; 2 = frame palette (game over); 3 = frame palette,
  brightness `D_80103610[slot]`; 4 = `func_8001CBE4`. All on OT layer 0, so slot 0 is on top.
- SD anim command 7 (`func_800302B0`) uploads a VNAO image: 0 → VNAO1 as page 26, 1 → VNAO2
  as 26, 2 → VNAO3 as 27, 3 → VNAO3 as 26. All three use the same VRAM, at (896, 256).
- Background (`func_800243E8`, drawn by `func_800418CC` on its own ordering table under the
  sprites): `DOMI_BG.OBJ` set 0 is a 6×4 map of 64-px cells, all from the 128×128 patch
  (0, 128) of PAPER.TIM (tpage `0x401F`, clut (256, 480)). The scroll moves by (+1, −1) per frame
  (`func_80024640`).
- Menu `func_80027950` (init) and `func_80027A64`; cursor `D_800B1400` (0 Arcade, 1 Insane,
  2 Time Trial, 3 Chain Reaction, 4 Option), sub-state `D_800B1340`:
  - state 0: both pads, pressed word == ↑ / ↓ moves with wrap (SE 0xA7); == Start or ✕
    confirms (`D_800C2FB8` = that pad). Description `D_80056C70[cursor]` (VNAO3 speech
    bubble) at (0x20, 0x50), layer 2. Confirm: SE 0xA8, then per item mode `D_801026DA` =
    6/4/11/7/17, the item's slot palette = 1, SE 0xA1/0xA2/0xA3/0xA4/0x9F, exit length 0x3C.
    Held word == 0x4C (L1 + R1 + ✕) on Insane, Chain Reaction or Option sets `D_80109638`
    (effect not traced). The arrow `D_80055C8C[frame]` (rest frame 5) at (0x82,
    `D_80100368[cursor]` = 0x43 + 0x20·i), layer 0 (in front of the SD objects), plays frames 6–31 after a confirm, then
    state 1 with script 0x106.
  - 1: after 20 frames the arrow flies off (x −16, y −2 per frame); △ → 2. At t ≥ 0x3C:
    mode 5 (script 0x108), 12 (0x10B), 9 (Insane and Chain Reaction, music off) or 17 (0x15F).
  - 2: arrow flies on; after 0x14 frames script 0x105 and the menu again.
- Mode 9: `func_8002B294` sets sub-state 3 and state 1; `func_8002B380` keeps the scripts and
  background for 0x3C frames, and at 0x40 plays the mode's voice (kind 3, id from
  `func_8002C708`: 0xA5/0xA6/0x9E/0x53) → sub-state 1. `func_8002B454` state 1: 2 frames,
  state 2 loads the game art (`func_80017744`, `func_800183C4` for solo), then 4 frames and
  mode `D_801026DA`. Its state 0 (ten pictures `D_80055BC4` on pages 0–2 with SE 0x12 every
  10 frames) is not reached from the menu.
- Sound kinds (`func_80040740`, `jtbl_80013AC4`): 1 play (loads the bank first if needed),
  2 and 5 `SsUtAllKeyOff`-style stop (`func_8004D7A4(0)`), 3 **load only** (`func_800408A0`:
  stops CD audio, swaps out the previous preloaded bank `D_800B13A4` if it differs, opens
  and transfers the VAB; plays nothing), 4 unload (`func_800409EC`).
- Mode 9's id (`func_8002C708` → `D_800B137E`): mode 4 → 0xA6 (NAMADERO), 6/13/14 → 0xA5
  (IYO_PON), 7 → 0x53 (EYECATCH), 11 → 0x9E (GRID), others unchanged. It is only *played*
  (kind 1, timer 0x8C) by state 0 `func_8002B630`, so menu → game is silent; the page needs
  nothing beyond `DEROMAIN` and `OUTGAME` for the front end. `examples/sedump.rs` lists the
  effect table with bank names.

## Arcade screens (verified, mode 5)

Implemented in `stackers_core::front::arcade` (drawn by `Screen::title`; `examples/title.rs`
takes `left`/`right` and pad-2 presses as `2cross@900`).

- Mode 5 (`func_8003A7DC`: scores and all-clear counts cleared, then `func_80015F28`, sub-state
  `D_800C34AA` via `jtbl_800100F0`: 0/1, 6/7, 8/9; then `func_800194D4`). Entered from the menu
  with script 0x108, palettes slot 0 = 3, 3 = 7, 1–2 = 0, 4–6 = 2 (every "back" into sub-state 0
  writes the same).
- 0 (`func_8002840C`): cursor `D_800B1400[pad]` = 0, arrow y `D_80100368` = {0x53, 0x83}, arrow
  at (0x48, 0x53), set choice `D_800C2FE6` = 0, `D_8010ADB0` = {1, 3, 5}.
- 1 (`func_8002855C`, state `D_800C2EC0`), menu pad `D_800C2FB8` only:
  - 0: t is held at 0x14 (so from frame 0x15 the set digit `D_80056A18[choice]`, LEVEL page,
    is drawn at (0x90, 0x98), layer 0); △ → state 2 (script 0x10A). ↑/↓ wrap over 2 items
    (SE 0xA7). On item 1, →/← step the choice (wrap 0–2, SE 0xA7) and `D_80103692` =
    `D_8010ADB0[choice]` (sets: 1, 3, 5). Start/✕: SE 0xA8; item 0 "Player 1 vs CPU":
    `D_800B0050` = 1 if the pad is 0 else 0, slot 1 palette 1, `D_801026DA` = 13; item 1
    "Player 1 vs Player 2 / Set Match": `D_800B0050` = 0xFF, slot 2 palette 1, mode 14. Both:
    `D_800B137C` = 0, `D_800662F0` = 0, `D_80100688` = −1. Arrow frames 5 → 0x1F, then state 1
    (script 0x109).
  - 1: digit while t < 0x28; arrow flies after 0x14 (x −16, y −2); △ → sub-state 0 (0x108). At
    0x3C: 0xFF → sub-state 8 (script 0x10B, palettes 0 = 3, 6 = 7, 1–5 = 0, 7–9 = 2), else
    sub-state 6 (0x10E "Practice course 3 stage / Normal Course / Hard Course", palettes 0 = 3,
    4 = 7, 1–3 = 0, 5–7 = 2; `func_800290F4`/`func_8002922C`, not ported).
  - 2: arrow flies; after 0x14 script 0x105 and mode 8.
- 8 (`func_8002A644`): both cursors 2, `D_80100368` = 0x43 + 0x20·i, arrows (0x28, 0x83) and
  (0xD8, 0x83) (player 2's mirrored), stocks `D_800FFCF8[p]` = 0.
- 9 (`func_8002A784`), both pads (`D_800B1908`, `D_800B190C`):
  - 0: `func_8001D778(0x40, 0xF0, 0x30)` with `D_80100674/6` = the stocks: per player the
    bomb label `D_80056CE0` (MSG2; `D_80056CF2` while that player is in state 12) at (x, 0x30)
    and the stock as one font-2 digit, palette 3, at (x + 0x10, 0x38) / (x − 8, 0x38); the digit
    is added first. A confirmed player's △ takes the choice back (its item palette goes back to
    0 unless the other player confirmed the same item) and the press is consumed. Then per
    player in state 0: ↑/↓ wrap over 5, → / ← stock ±1 within 0–3 (SE 0xA7 only on a change),
    Start/✕ confirm; after 0x14 frames △ with both unconfirmed → state 2 (script 0x10D).
    Confirm: SE 0xA8, item palette 1, `D_80102740[p]` = item + 1. Both done → state 1 (0x10C).
  - 1: arrows fly apart after 0x14 (x ∓16, y −1); △ → sub-state 8; at 0x50 → mode 9, music off.
  - 2: arrows fly; after 0x14 → sub-state 0.
- Sprite calls: `func_8001B860(frame, x, y, mirror, scale, layer, clut, brightness)`; the arrows
  use layer 0 and clut index 3, and are added before the SD VM runs, so they (and every
  overlay above) are in front of the SD objects. `func_8001B148(frame, x, y, mirror,
  brightness, layer)`.

## Set match (verified, mode 14)

Implemented in `stackers_core::set_match` (rules only; the screen is not ported yet).

- Mode handler `func_80015FCC` (modes 14 and 15), sub-state `D_800C34AA` (`jtbl_80010118`):
  0 init, 1 play, 2 set result `func_8003D184`, 3 pause `func_8002E59C`, 4 match end
  `func_8002D114`, 12 re-pick `func_8002B8EC`.
- Init: `func_80039DE8` takes each player's level from `D_800120D4[D_80102740[p]]`, a row of
  (colours, starting garbage `D_800C34E4`, quota, speed index):
  1 (4, 0, 0x20, 3), 2 (4, 0, 0x20, 5), 3 (5, 0, 0x20, 7), 4 (5, 0x12, 0x20, 9),
  5 (5, 0x1E, 0x10, 0xC). `func_8001FCA0` puts garbage / 6 into pending[1–6], and the bomb stock
  `D_80100674[p] = D_800FFCF8[p]`. The speed-up is the quota one (`func_80039AC4`) and the
  garbage divisor runs two steps ahead. HUD init `func_8003B958`: indicator column
  `D_800C3132` = 1 (special column 2), script 0. "Ready Go" is script 0.
- Bomb: in `func_8001F194`, before the fall, □ (`D_800C34F8[p] & 0x80`) with stock left sets
  player state 12 and `D_800C34E8[p] = 1`. After both cell updates, `func_80038A08` →
  `func_80038C0C` starts the three cut-in scripts with entry 15 (SD_CHR tables
  `0x801A8240/8280/82C0`), SE 0xA9, `D_800C34E8 = 2`. The scripts' end op sets 0xFF after
  44 frames. State 12 (`func_8001EAB0`) then runs `func_800201C4`: every non-empty cell with a
  colour ≠ 0 in rows H−2 → 0, columns 1–6, gets excited (+0x2A = −1). Then stock −1, state 7,
  `D_800C34E8 = 0`. The pair in hand is dropped. From the press to state 7 takes 46 frames.
- Indicator (`func_8003BD78`, scripts from `BLT_ANM.OBJ` via `0x801C0044`): script 0 idle bob,
  1 op 3 every 6 frames (column +1, wraps 6 → 0, SE 0x19), 2 flash then 0. `func_8003C340`
  starts 1 when the player's chain value goes 1 → 2 (`D_80101EC8` holds last frame's).
  `func_8003C3D8` switches to 2 on state 4, on ○ / ✕, or at the set's end (`D_80069694`). The
  flash length is approximated as 6 frames.
- Pause: Start (alone) on either pad while the clock runs. `D_8010ADAE` = that pad, and only it
  drives the menu. `func_8002E514` / `func_8002E79C` is the solo menu with Continue / Quit only
  (cursor wraps over 2). States: 0 select, 1 quitting (title after 0x3D), 2 "Quit?"
  (✕ / △), 3 continue.
- End check `func_8002DD28`: player 1 in state 10 → result 1; player 2 → 2; both → 2.
  Result 1 means player 2 won (`D_800B1238 = 1`, wins `D_8010ADFE` +1), and result 2 means
  player 1 won. If the winner has `sets / 2 + 1` wins, their match tally `D_800699C4/6` goes +1
  (cap 999) and the game goes to sub-state 4; otherwise to sub-state 2. `func_8003B2B8`: the
  loser's field bursts, and in the winner's field (`func_8003B7E0`) every blob turns colour 6 /
  state 13 and pops, with SE 0x1E. Both players go to state 10.
- Set result (sub 2, timer `D_80066ACC` from the end): phase 0 steps the burst / pop. At 0x5B
  it shows script 0x16 (player 2 won) or 0x17 (player 1 won). From 0xC9, ✕ or Start on the
  loser's pad (`(D_800B1238 + 1) & 1`) starts the next set (sub 0). At 0x169 the next set
  starts anyway.
- Match end (sub 4) is the solo game-over sequence with scripts 0x45 (player 2) / 0x46
  (player 1), and its countdown answers the loser's pad. Start → phase 4 → after 0x3C,
  music off, wins reset, then sub-state 12. When the countdown runs out the game goes to the
  title.
- Re-pick (sub 12): script 0x42; both cursors start at 2, both stocks at 0. After 0x14 frames
  each pad runs: ↑ / ↓ difficulty with wrap over 5, → / ← bombs 0–3 (SE 0xA7). ✕ or Start
  confirms (SE 0xA8, difficulty = cursor + 1) and blinks for 0x1B frames, then the player is
  ready (△ goes back). With both ready: script 0x43, the lists fly out (±0x10 px per frame for
  0x15 frames), and after 0x50 frames the game goes to sub-state 0. The list drawing is not
  ported.

## Chain Reaction (mostly verified, mode 7)

Core rules ported (`Rules::ChainReaction`, `Game::chain_reaction`, `stackers_core::camera`);
screen in `stackers_formats::chain` + `Screen::chain` (`examples/screen.rs` modes `chain`,
`chainzoom`, `chaindrop`); the menu's Chain Reaction launches it (mode 9 → 7). Handler `func_80015AAC`, sub-states `jtbl_800100D8`:
0 init, 1 play, 3 pause, 4 end (`func_8002D628`, entered by `func_8002DC58` with
`D_800B1238` = 1: never a new record).

- Init `func_80015ED4`: height `D_800FFD00` = 79, queue x `D_800B0F90` = 0x98, clock cleared.
  Then `func_80039CEC` (row 3: 5 colours), `func_8001FCA0`: gravity `P+8` = 0x80010, soft drop
  `D_800C3038` = 0x140001 (else 0x80001). Camera −16, scale 0x80, camera state 1,
  `D_801037E0` = record `D_80067224`. No speed-up at spawn.
- Preset (`func_80027098(0, D_80100680 − 1)`) only when `D_800C34AC` is 0x120 (→ 1) or 0x140
  (→ 2), apparently a secret; it also disables soft drop, □ and the record.
- □ (pressed 0x80, player state 0, `P+0x1A` = 0, after the fall step): `func_800200F0` sets both
  colours to 0 and locks the pair (`func_8001F67C`) → state 1.
- Lock record `D_800C3000 + 0x18·p` (`func_8001F67C`): pivot col, row, colour, y; satellite
  col, row, colour, y.
- Zoom `func_8001B034` (held): L2 −1, L1 +1, both → 0x80; clamp 0x40…0x80 (`D_800FFDB8`).
- Camera `func_8001AA10` (`D_801095B0`, table `0x800103D4`; idle counter `D_80069654`): see
  "Intro" above; state 2 △ (not paused) adds 0xF0 + (0x80 − scale)·240/128 and wraps to −16
  when it started at 0x3B0; state 3 camera = max(pivot y, satellite y) − 0xC8, never above
  its value on entry; pivot colour ≠ 0 and pivot y + 16 = satellite y → 4.
- Chains `func_8002C770` (all modes): chain value change to ≠ 0 → `D_800B1380` += 1 (SE
  9 + old, max 17); change to 0 → if length ≥ `D_801037E0`: best = length and (mode 7) record
  `D_80067224` = length; length reset.
- HUD `func_8002C98C`: `D_800B1380` and `D_80067224` as 3 digits each (zeros shown), font 3
  (`D_80056388`), x 0xD8/0xF0/0x108, y 0x28 / 0x70, layer 1.
- Frame `func_8001A2B4`: tower `D_80055B44` records 0–4 (JIYU12/34/55, 128×256; record 4 reads
  past JIYU55's 176 rows) at field (−1, −0x18 + 0x100·i) (`func_8001A270`), i.e. screen
  (7, y − camera + 16), zoomed like the field, drawn while y ∈ −0x103…0x1F3, layer 3, scale + 1;
  record 5 (top edge) at segment 0's position on layer 1; WINDOW `D_80055BB0` at (0x8C, 0x10)
  layer 1; panels `func_8001A560(0, 1, 2, 5, 2, 10, 0)` (all its sprites on layer 2, labels
  added before monitors). `func_80023294` skips the solo queue patch in mode 7.
- Zoom (`func_800195FC`, also `func_80019998`, `func_8001A2B4`): screen = c + (p − c)·scale/128
  in px << 12, c = (`D_800C30B8` = 0x48, `D_800C30C0`). `D_800C30C0` is recomputed each frame
  from the camera: < 0xE0 → 104·cam·4096/240 + 0x16EEE (16 at the top), 0xE0…0x2C0 → 120,
  above → 104·cam·4096/240 − 0xB9111 (224 at 0x3B0): the top of the tower stays pinned to the
  top of the screen when zooming out, and the bottom to the bottom. Blobs are drawn while the
  zoomed y ∈ −19…239.
- Pair (`func_80019998`, called from the field draw when the player state is 0 or 3, all
  modes): pivot at x − 8·sign(`P+0x1A` move anim), y − (`D_80100618` >> 12) + 8·(flip cd < 0)
  − 8·(flip cd > 0); satellite from the same x and y ∓ 10, plus the rotation offset
  (`D_800570A8`) turned by `RotMatrixZ(P+0x14 · 128)` (the rotate cooldown eases it round).
  `D_80100618`: a gravity step of n px (`func_8001F4D4`) sets it to n << 12 and
  `D_80100620` = (n << 12)/delay; every gravity-path frame (`func_8001F41C`) subtracts the
  rate (floor 0); soft drop and landing zero it. **Camera follow:** while camera state is 1,
  camera = pivot drawn y − 0xC8, except a rise of less than 0x20 is ignored; then clamp.
  This runs in the draw, after `func_8001AA10`, and also while paused.
- Mode 7 extras in `func_80019998`: off screen (zoomed y < −0x13 or ≥ 0xF0) → arrow
  `D_80056D04[f]` at (x, 0x10) or `D_80056D84[f]` at (x, 0xE8), zoomed, plus `D_80056D04[f]`
  at (0xF4, 0xD0) scale 0x40; f = (`D_80066AD0`/9) % 6. Always: the pair in the "Falling Blocks"
  panel (pivot at (0xF0, 0xC0), satellite offset as above with flip ∓ 0x12, unzoomed) and the
  depth marker `D_80056E04[f]` at (0x128, 13·`P+4`/72 + 8), scale 0x40.
- Background: `func_8002430C(D_800662F0)` = DOMI_BG set 0 (the only set): the title's paper
  layer, kind 2, unscaled and never scrolled in mode 7. `func_8002466C` would tie a layer of
  kind p to player p's camera/zoom (GsSortBg with scale `D_800FFDB8` << 5), unused here.
- Arm spans (`func_800258A8`, `D_800C3538`) are shared with every mode: see "Arm spans".
  They matter most here, since □ drops grey pairs and arms reach over grey blobs.
- No start banner: READYGO2 (page 27) is loaded, but mode 7 never requests SD script 0 (the
  "Ready … Go" of the two-player modes, `func_80015FCC` → `D_800C3514 = 0x100`) and only runs
  SD scripts in sub-state 4. `func_800194D4` just sorts the frame's ordering tables.
- Mascot (`func_80031730`, mode 7 only, every sub-state; `stackers_core::mascot`,
  `sdchr::Vm::Actor`, drawn by `Screen::chain`): a girl in a bath at the foot of the middle
  column. Init (`func_80031828`, `D_8010AE00` 0 → 1) points both actors at the SD_CHR table
  `D_801A8308` (other modes: `D_801A8200[D_800662F0]`) and starts mood 0; mode 7 then only
  steps and draws actor 0 (`D_800699D0` = 0xFF, `D_800699D2` = 0).
  - Mood (`func_8003189C`, not while paused): chain `D_800C3068` ≥ 2 → `D_8005F63C` = play
    frame; mood = (`D_8005F63C` + 18000 < play frame), i.e. she dozes off after 5 minutes
    without a 2-chain. A change restarts actor 0 with `table[mood]` (`func_80031A40`).
    Moods 2–11 exist in the table (for the two-player selectors `func_80031D08` etc.) but
    repeat mood 0 here.
  - Actor VM (`func_80031910`: per object, unless paused, motion `func_80032180` + animation
    `func_80032590`; then draw `func_80032934`): 4 objects × 0x48 at `D_8005F3E4`, same layout
    and motion ops as SD_CHR (`jtbl_80011EEC`, no op 7). Animation commands `jtbl_80011F0C`:
    0 jump, 1 set the actor's done flag `D_8005F3D4`/`D_8005F3D8` and stall, 2 layer, 3/6 hang,
    4 scale tween, 5 SE, 7 upload (`func_80032BCC`: 0/1/2 → `0x80180000`/`0x80189000`/
    `0x80192000` = VNAO1/2/3 as page 26 (`D_80103590`), 4 → VNAO3 and 6 → VNAO1 as page 27
    (`D_801035B0`), 3/5 → `0x8019B000`, never loaded). Draw: `func_8001B148(frame, x + 160,
    y + 120, flip, sx, layer 0)`; when scaled, x −= (sx − 128)/2·100 / (0x3200 / w) and y
    likewise with h.
  - Mode 7's scripts: VNAO2 as page 26, position (−24, 24) → frame top-left (140, 136).
    Mood 0 = the 56×72 frames u 0/56/112/168 v 0, 12 frames each; mood 1 = 64×72 u 0/64 v 72,
    24 each.
  - Voice lines (`func_80032CCC`, bank `ASA_DERO`, kind 1): counters `D_8005F648`/`F64C`/`F650`
    +1 per call. Chain ≠ 0 and ≠ last answered (`D_8005F660`): chain 1 nothing, 2/3/6/7 → 0x77,
    4/5/≥8 → 0x7D. Chain 0: after an hour / half hour the counters just reset (nothing plays);
    else once `D_8005F648` > 0xE10 it resets and `rand() & 3` picks 0 → 0x76, 1 → 0x78, else
    nothing. While paused, one `rand()` on the first paused frame (`D_8005F658`). After game
    over (`D_80069694`, set by `func_8002DCD8`) once: record `D_80067224` in 20–29 → 0x7D
    (nothing for ≥ 30 or < 20, as written). Muted while paused or with the menu secret
    `D_80109638` (not modelled); presets skip it. These `rand()` calls are the piece RNG, so
    they shift the piece sequence.
- Art `func_800179A8`: PSFONT → 12, JIYU12/34/55 → 18/19/20, WINDOW → 23, READYGO2 → 27,
  DOMI_BG → `0x80160000`, FIL2_DAT (presets) → `0x80170000`; file table `D_80057298` index
  0 FIL2_DAT, 2 DOMI_BG, 7 PSFONT, 0xB READYGO2, 0x2B–0x2E JIYU12/34/55/WINDOW.

## Garbage (verified, two-player modes)

Implemented in `stackers_core::garbage` (tables in `tables.rs`, checked against the exe by
`stackers-web/tests/garbage_tables.rs`) and `stackers_core::Versus`.

- Counters per player, i16 by column (0 = unassigned pool, 1–6 = columns): incoming
  `D_801003B8[p][7]` (stride 0xE), pending `D_801006A8[p][7]`.
- Sending (`func_8003A82C`, once per frame after both cell updates): if p0's attack points
  `D_80066206` ≠ 0, score += points · 100 and `D_801003B8[1][0]` += points / `func_8003AE70()` + 1
  (also counted in `D_801005EE`, the blobs sent this turn); p1's `D_8006620A` → `D_801003B8[0][0]`,
  `D_801005F4`. Points only exist from chain 2 on (see "Matching"), so a single clear sends nothing.
- Divisor `func_8003AE70`: `D_80057680[min(D_80066AD0 / 900 (+2 in mode 14), 32)]`: 70 for the
  first 90 s, then 60, 47, 35, 28, 23, 20, 18, 16, 14, 13, 12, 11, 10, 8, 8, 6, 6, 3, 3, 2, 2, 2,
  1 … (one step per 15 s of play).
- `func_800239A0`, at the start of each player's cell update (before the queue step):
  1. `func_80024034`: if the pool is non-zero, total = pool + Σ(pending + incoming) over columns
     1–6, share = total / 6; every column with pending < share gets incoming = share − pending
     (overwritten, not added); then `func_800241D4(total % 6)` and pool = 0.
  2. `func_800241D4(n)`: record `0x80057152 + 22·n` = (count, count for 4 columns, 20 masks);
     `rand() % count` picks a mask, bit b → incoming[b + 1] += 1. The data has quirks, kept as is:
     n = 4 lists only 6 four-bit masks then `0x3F`, so 7 of 13 picks add a full row (6 blobs);
     n = 5 reuses the four-bit masks (4 blobs); n = 2 repeats `0x14` instead of `0x11`.
     A 4-column variant (`func_80023EFC`, columns 2–5) is only called from unreferenced code at
     `0x80023D7C`.
  3. If the **opponent's** player state is 11 or 0 (its turn is over), incoming[1–6] are added to
     pending and cleared.
  4. If this player's state is 4 (the turn ended, about to spawn): `D_80100640[p]` = 0, then for
     columns 1–6, up to 5 blobs: before the k-th blob (k = 0…) the cell at row 6 + k is checked;
     if it is settled (state 2) the rest of the column goes back to incoming. Otherwise
     `func_80022908(c, 5 − k, 1, colour)` with pending state 1 (falls in). Colour 6 if
     c − 1 == `D_800C3132[p·0x18]` or the mode is 11, else 0 (grey). Placed counts go to
     `D_80100640[p]`, `D_801005FA[p]` and `D_800B001C[p]` (the grey-landing sound gate).
- `D_800C3132`: the special-column indicator of the two-player HUD (struct `D_800C3110`, stride
  0x30). Init 1 (`func_8003B958`); stepped by the HUD script in `func_8003BD78` (op 3: +1, wraps
  6 → 0, SE 0x19, the indicator sprite from `D_800557C4`). Ported for mode 14 as
  `set_match::Roulette`, which feeds `Game::special_column`.
- All clear (`func_8003AA78`, modes 11, 13, 14, 15): if a player's empty-cell count is 0x6C (the
  whole 6×18 area) and `D_800B1234`/`D_800B1236` is 1 (setter not traced), the blobs sent this
  turn are sent again, the all-clear count `D_800B138C[p]` += 1 and score +=
  `D_800576C8[min(count, 7)]` (10,000 … 1,200,000). Not ported.
- Pieces (`func_80022F14`): when both colour counts `D_800C3534[2]` match, pairs come from the
  shared ring `D_80100318[32]` (write head `D_8006626E`, read heads `D_800B0054[p]`): a player that
  has read everything rolls two colours into the ring first (satellite then pivot). Player 1's
  initial queue is player 0's (`func_80023474`, no `rand()`). Otherwise `func_8002319C` rolls.

## Time Trial (verified, modes 12 → 11)

Implemented in `stackers_core::trial` (`TimeTrial`, `CourseSelect`), `Versus::time_trial`,
`Rules::TimeTrial`, mode 12 in `stackers_core::front`, `stackers_formats::presets` (FIL2_DAT)
and `stackers-web` (`Trial`, `Screen::trial`; `examples/screen.rs` modes `trial`,
`trialfill`, `trialpause`, `trialselect`).

- Menu item 2 → mode 12 (script 0x10B; palettes slot 0 = 3, 6 = 7, 7–9 = 2, 1–5 = 0) → mode 9
  → mode 11 (`func_80016BB4`, sub-states `jtbl_80010180`: 12 in-game course list
  `func_8002C16C`, 0 init, 1 play, 3 pause `func_8002DE14`, 4 end `func_8002D628`).
- **Mode 12** (`func_80016B5C`: `func_80029B5C` init, `func_80029CA8`): cursor `D_800B1400[pad]`
  starts at **2**, arrow `D_80055C8C` at (0x28, `D_80100368[cursor]`). State 0: after 0x14
  frames △ → state 2 (script 0x10D). ↑/↓ wrap over 5 courses (SE 0xA7); Start/✕ → SE 0xA8,
  slot cursor + 1 palette 1, course `D_800699D4` = cursor, `D_80100688` = −1 (forces the art
  load); arrow frames 5 → 0x1F, then state 1 with script 0x10C. State 1: after 0x14 frames the
  arrow flies (x −16, y −1); △ → mode 12 again (0x10B; this path writes 2 into slots 8–10);
  at 0x50 → mode 9 with `func_80040A7C(1, 2)` (music off). State 2: arrow flies, after 0x14
  script 0x105 and mode 8. Course names are SD script objects on the title's LEVEL page.
- Courses: `FIL2_DAT.OBJ` (file 0, RAM `0x80170000`, loaded by `func_80018518`): u32 pointer
  per set, each set a pointer table, layouts back to back. Set 0 = 12 Chain Reaction layouts of
  72 rows; set 1 = the 5 courses (Easy, Normal, Tough, Hard, Insanity), 12 rows × 6 i16
  (0x80 = empty). Course 2 ("Tough") is an empty field.
- Init (sub-state 0): `func_80017044` (height 19, queue x 0x93, clock and `D_80066AD0` cleared,
  `D_800662F0` = 0); art (`func_800185D4`, only when `D_80100688` ≠ `D_800662F0`): ST53B → page
  18, OJAWAKU2 → 21, LEVEL2 → 17, ST53A → RAM `0x80138000` (background), RENSA1/2 → 28/29;
  mode 9 already loaded NULL, VS_WIN2 → 20, W_ZERO → 24, FIL2_DAT. Then `func_80039CEC` (row 3
  for both players, like every non-13 mode), `func_8001FC64` (both players; mode 11 intro
  `D_80068510[p]` = **0xB4**), `func_80018A08` (both cameras −16, scale 0x80),
  `func_80027098(1, course)` into player 0.
- Speed-up: `func_80039A78` → `func_80039AC4` (quota 40, row 3), as in mode 13.
- Two players: `func_8001E07C` runs `func_8001E0B8` (p0, state handler + controls) then
  `func_8001E148` (p1, state handler only: **no controls, no gravity**). P1's state 11 handler
  (`func_8001E910`, mode 11, p = 1) never spawns: spawn cell (3, 6) not settled → state 4 (takes
  garbage again); settled and `D_800662D8` (p1 settled count) ≥ 0x48 → 10; settled sooner →
  state 0, where p1 is stuck (as written). P1's queue isn't drawn (`func_80022A5C`).
- All garbage is colour 6 in mode 11 (see "Garbage"); colour 6 never matches, so p1 just fills.
- Clock `D_80100648` (`func_8003D8D8`): in mode 11 a minute overflow sets it to `0x3B3B3B`
  (stops at 59'59"59).
- Pause entry as in mode 4 (Start alone while the clock runs). Sub-state 3 draws the frame, the
  queue and **only player 2's field** (player 1's is hidden), then the pause menu; Retry →
  sub-state 0 on the same course, Quit → title.
- End `func_8002DA70` (after each play frame, after the pause check): `func_8002DCD8` p0 in
  state 10 → `D_80069694` = 1, `D_800B1238` = 1. Else p1 settled ≥ 0x48: best
  `D_800C30D0[course]` = 0 or clock < best → best = clock, `D_80069694` = 2, `D_800B1238` = 0;
  else 1/1. Both: clock flag off, limit 0x12C, sub-state 4, `func_8003B2B8`, which in mode 11
  sets p0 state 10 and either (record) `func_8003B54C(1)`: every colour-6 cell of p1 → state 13
  (its pop script) + SE 0x1E, then each frame `func_8003B644(1)` empties them when the script
  ends; or bursts both fields (`func_8003B3DC(0)`, `(1)`; `func_8003B140` steps both).
- Sequence `func_8002D628`: result script by `func_8002D91C` (index mode − 2; mode 11 = mode 4:
  0x113 record "You Win", 0x114 otherwise), countdown as in mode 4; phase 4 (Continue) in mode
  11 → **sub-state 12** instead of a restart.
- In-game list `func_8002C16C` (state `D_800699C0`, timer `D_800B19A4`): 0 → script 0x142,
  cursor 2, list palettes slots 0–4 = 0, 5–9 = 6, arrow (0x28, 0x83) frame 0 (returns before the
  VM); 1 → after 0x14 frames state 2; 2 = the same menu as mode 12 (slot = course palette 1),
  arrow `D_80056A88[0…0x1A]` (LEVEL2, page 17, palette 3, layer 0), then state 3 with 0x143;
  3: △ → sub-state 12 state 0; arrow flies after 0x14; at 0x50 → sub-state 0. Drawn: frame
  (`func_8001A70C`) and the scripts (draw mode 1), no fields or clock.
- Screen (`func_8001A70C`): `0x80056820` (W_ZERO, page 24) at (0, shake) layer 1, monitor
  `D_8005666C[0]` at (0xC8, 0xA0) palette 2 semi-transparent layer 2, OJAWAKU2 frame
  `0x8005680C` at (0xC0, 0) layer 2. Field draw `func_800195FC(n)`: x + 0xB8 for n = 2 (player
  2's field, its own camera/scale `D_800667AC`/`D_800FFDBA`), else + 8. The shake
  `func_8001A8B8` (from `D_80100408`, table `D_80056EC0`, SE 0x1F above 16) stays 0 here.
  Queue patch `D_80055F14` comes from ST53B.
- Clock HUD `func_8003D9B0(clock, best)`: " ", minutes (2, zeros), "kS", seconds, "kD", then
  frames / 6 and (frames % 6)·2 capped at 9 (1 digit each), font 1; clock at (0xD0, 0xB4)
  palette 7, best at (0xD0, 0xCC) palette 1.
- Music: index 9 (Track 11). Not ported: `func_80038A08` (a HUD event state machine,
  `D_800C30B2`), the 2P mascots `func_800312AC`, RENSA effects, mode 9's voice.

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
| 4 | an arm started reaching this frame (`D_800FFDBC`) | `func_800248F4` |
| 8 | an arm poked a grey blob | `func_8002602C` |
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

- What fills pages 0–2 for mode 9's state-0 picture sequence (`D_80055BC4`), and which path uses it.
- The `D_80109638` secret.
- Which mode is 13; what `D_800C33F8` and the per-level `D_8005588C` records are.
- Scoring display, special blocks (colour 6) behaviour, CPU AI, the special-column HUD script.
- What plays on game over (XA table users); title = CD-DA index 0, menu keeps it, mode 9 stops it.
- SD_CHR motion op 1/7 with n = 0 (`func_800324B4`).
- `func_80026E8C` (after each span step): counts `D_80066D3C` down, playing sound 6 at 1 and
  5 at 0x10. Who sets it is not traced.
