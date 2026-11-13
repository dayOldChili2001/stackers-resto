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
| 7 | clearing (grey next to a match, or under a cleared arm) | `func_80021030` | as 4 |
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
- Each group ≥ 4 is cleared: in `func_80033DFC`, every blob of the group gets pending 4, the
  clear timer (`func_80034C3C`) and `func_80034994`: its settled grey neighbours (below, above,
  right, left; one word compare, state 2 and colour 0) get pending 7, so **grey next to a match
  clears with it**, then every neighbour whose colour is not 0, 0x80 or 0xFF is excited, the
  group's own blobs included (so they are excited from the match, not only once a neighbour's
  clear ends in `func_80020C58`). Found by `tools/framediff.py` (see below).
- If any group cleared: `chain += 1` (`D_800C3068`, reset at spawn),
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
the mode inits): colours → `D_800C3534/6`, variant → `D_800B1398/13A0` (selects the CPU record
per level from `D_8005588C`, copied to `D_800B13E8/13F4`; see "CPU"), quota → `P+0x2C`,
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
- **Erase Number** `D_8010040C`: `func_8002CBC0` snapshots the blob counts (`D_80100634`
  coloured, `D_801005FA` grey; see "Blob counts") before the player update, `func_8002CBE8` adds
  each count's drop (if positive) after the cell update; capped at 99,999,999.
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
    9 … 0). Start: SE 0x1E, → 4. Reaching 0x22 ("Game Over"): limit 200, XA 7 (`GameOver::xa`), → 5.
    The timer and ✕ each add 0x101 to the request `D_800C3514`, so both on one frame make a
    0x2xx request: the digit after next, started with placed records (`GameOver::placed`). The
    Game Over check compares the request with 0x122 exactly, so a skip onto 0x22 misses it:
    "Game Over" shows (placed) but the countdown runs on through scripts 0x23…0xFF, wraps
    (0xFF + 0x101 = 0x200, script 0 placed) and only ends when a single step next lands on
    0x122 (`over::tests`). In practice it never gets that far: 0x23–0x27 run (junk), but 0x28
    is not a script and its records reach motion op 0, the endless loop `func_8002F278`
    (`j` to itself), so **the console freezes** unless Start is pressed first (found by the
    end diff, `FRAME_SEED=9090` game 4, frame 5,921). The port stops that object instead
    (`sdchr::motion_op`), as for any op 0.
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

## Solo mascot (verified, mode 4)

Implemented in `stackers_core::mascot` (`Kind::Solo`, `Game::solo`), `SOLO_ACTOR` in
`stackers_formats::sdchr`, `Session` and `Screen::solo`; test `solo_mascot_scripts` (real disc).

- `func_80031590` runs in every mode 4 sub-state (0 init, 1 play after `func_8002CC88`, 3 pause,
  4 game over). Sub-state 0 sets `D_800699D0` = 0xFF, `D_800699D2` = 0, `D_8010AE00` = 0; the
  call then runs `func_80031660` (mood 0, both actors at the SD_CHR table `D_801A8304`, start
  them) and zeroes `D_8005F648/F64C/F650/F644`, `D_8005F658/F65C/F664`. Later calls:
  `D_8005F648` +1 (only that counter), then, unless paused (`D_800C34AA` = 3), the mood
  `func_800316C4`; then `func_80031910(0)` (step + draw actor 0, layer 0, added after the
  counters so behind them) and the lines `func_80032FF0`.
- Mood from the chain `D_800C3068`: 0–1 → 0, 2–3 → 1, 4–5 → 2, ≥ 6 → 3; a change restarts the
  actor (`func_80031A40`). Scripts (VNAO2 as page 26, uploaded by mood 0's script only; the
  others draw from it): mood 0 32×72 u 128/160 v 72, 8 frames each; mood 1 adds u 192 v 72 and
  u 0 v 144; mood 2 48×72 u 32/80/128/176 v 144, 4 each; mood 3 = mood 2 at scale 170. All at
  (−24, 24), so the top-left is (136, 144) unscaled, under the queue, behind the score.
- Lines `func_80032FF0` (bank `NEW_YORK`, slot 18, programs 60–67 = SE 0x8B–0x92): chain ≠ 0
  and ≠ last answered (`D_8005F660`): `jtbl_80011F6C` on chain − 1: 1 nothing, 2/3/6/7 → 0x8C,
  4/5/≥ 8 → 0x92. Chain 0: the hour / half-hour branches of mode 7's version are present but
  their counters never count here; else once `D_8005F648` ≥ 0x655 it resets and
  `jtbl_80011F8C` on `rand() & 0xF`: 0/3 → 0x8B, 1/2/4 → 0x8D, 5–15 nothing; answered = 0.
  Pause: one `rand()` on the first paused frame (`D_8005F658`). After game over
  (`D_80069694`), once (`D_8005F664`), by the erase number `D_8010040C`: < 1,000 → 0x8B,
  5,000–9,999 → 0x92, otherwise nothing (as written). Muted while paused or with
  `D_80109638`. These `rand()` calls are the piece RNG: an idle stretch of 0x655 frames shifts
  the pieces.

## Pause (verified, mode 4)

Implemented in `stackers_core::pause` and `Screen::solo`.

- Entry (`func_80017098`, sub-state 1, after the play update, before `func_8002DBAC`): while the
  clock flag `D_800667A4` is set, if the pressed word `D_800B1908[D_800C2FB8]` is exactly 0x800
  (Start alone) or either port's connection state changes (status bytes `D_800B1440` /
  `D_800B1462` vs `D_8006626C/D`, which the main loop `func_80014634` saves at the end of every
  frame in every mode; plugging a pad in counts too), sub-state 3 and `func_8002DD80` (menu
  init); `D_8010ADAE` = the menu pad. Game over in the same frame wins. Modes 7, 11, 13 and 14
  run the same port check after their Start check (see each). The port takes the change as
  `port_change` before each step; play.html counts a port as connected while a gamepad is
  assigned to it (the browser reports one from its first button press). Mode 13 has its own handler; modes other than 4, 7 and 11 draw only the plain
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
    or mode 0 (title). The handler's Start check runs after the menu and overwrites the
    sub-state with 1, so Start on exactly that frame cancels a Retry: play resumes with the
    music stopped (`Pause::music_stop`). Quit still leaves (mode 0 ignores the sub-state).
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
  13, 14/15, 16 ending (`func_80030798`, STAFF*.TIM), 17 Option (`func_8003DDD8`, see "Option").
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
    music stops → 5; 5: after 4 frames, load the game art and mode 2 (see "Attract demo").
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
    (cleared only by title init `func_80027240`, so it survives Option → menu). Its only
    readers are the solo and mode-7 mascot tails (`0x80032F58`, `0x80033270`): it zeroes the
    pending line `D_8005F654`, so the mascots say nothing. Moods, counters and their `rand()`
    rolls are unchanged, and `D_8005F644` is not reset (no line was played). The arrow `D_80055C8C[frame]` (rest frame 5) at (0x82,
    `D_80100368[cursor]` = 0x43 + 0x20·i), layer 0 (in front of the SD objects), plays frames 6–31 after a confirm, then
    state 1 with script 0x106.
  - 1: after 20 frames the arrow flies off (x −16, y −2 per frame); △ → 2. At t ≥ 0x3C:
    mode 5 (script 0x108), 12 (0x10B), 9 (Insane and Chain Reaction, music off) or 17 (0x15F).
  - 2: arrow flies on; after 0x14 frames script 0x105 and the menu again.
- Mode 9: `func_8002B294` sets sub-state 3 and state 1; `func_8002B380` keeps the scripts and
  background for 0x3C frames, and at 0x40 plays the mode's voice (kind 3, id from
  `func_8002C708`: 0xA5/0xA6/0x9E/0x53) → sub-state 1. `func_8002B454` state 1: 2 frames,
  state 2 loads the game art (`func_80017744`, `func_800183C4` for solo), then 4 frames and
  mode `D_801026DA`. Its state 0 is **dead code**: ten pictures `D_80055BC4` on pages 0–2
  with SE 0x12 every 10 frames and the mode's voice (`func_8002B630`), for 0x168 frames or until
  Start / ✕ on the menu pad (`D_800B1908[D_800C2FB8]`, checked every frame), then state 1.
  Every entry into mode 9 — the menu (`func_80027A64`) and `func_8002922C`, `func_80029CA8`,
  `func_8002A784` (they also zero the state) — sets sub-state 0 (`D_800C34AA`), whose handler
  `func_8002B294` (dispatch `func_80015A24`) always sets state 1, and nothing in mode 9 sets
  it back to 0. So the pictures never show and what pages 0–2 would hold is moot; not ported.
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
    4 = 7, 1–3 = 0, 5–7 = 2; `func_800290F4`/`func_8002922C`, see "vs CPU" → Course list).
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

Implemented in `stackers_core::set_match`, `stackers_web::Match` and `Screen::set_match`.
`examples/screen.rs … match` renders it (`matchbomb`, `matchchain` for the cut-ins, `matchspin`
for the strip, `matchrepick` for the re-pick lists). Music: see "Music director" (track 6). There are no
mascots: `func_80015FCC` zeroes both actor flags `D_800699D0`/`D_800699D2` before its
`func_8003121C` calls, so they only initialise. (Mode 13 `func_80016480` enables one of them,
and `func_80032934`'s tail, another caller, enables actor 1.)

- Screen (`func_8001A82C` + `func_8001DB4C`): background ST18A (file 0x55, when `D_800B137C` =
  0; ST27A / ST28A with RANNY1_A / RANNY2_A otherwise). Pages: TAISEN_A 18, VS_WIN2 20,
  W_TAMA 24 (`func_80017B20`), READYGO 27, RENSA1/2 28/29 (`func_80018E68`). Arcade's level
  page sets stage `D_800662F0` = 0, so the SD VM's images are VAMIGO13/23/33 (files 0x89–0x8B,
  uploaded to page 26/27 by the VM, `func_800302B0`). Frame `0x80056820` record 0 at (0, y),
  record 1 mirrored at (0xBF, y), layer 1. Fields as in mode 11 (player 2 at x + 0xB8). Queues
  at x `D_800B0F90/2` = 0x88 / 0xA8 (`func_80016B18`), with slot 0's patch as in solo.
  HUD: sets (font 1, palette 5) at (0x78, 0x80), with "SET" at 0x84 and "MATCH" at 0xA0 (text
  font `0x80056844`: letters A–Y are records c − 'A', advance = record 0's width). Wins (font 1,
  palette 3) at (0x88 / 0xB0, 0x90), with "k-" (`0x800562F6`, palette 2) at 0x98. Match tallies
  (font 1, palette 3, zeros) at y 0xB0, right-aligned at 0x8C / 0xAC minus 4 per extra digit.
  Then `D_80100690` (`func_8001DA4C`: TEAFONT (0x60, 0x70) 16×16, a sideways "WON") at
  (0x88 / 0xA8, 0xC0), palette 0 (`hud::VS_TALLY_MARK`). Stocks
  (`func_8001D778(0x80, 0xB0, 8)` in `func_8001E020`): the bomb icon `0x80056CE0` record 0 for
  both players, with the digit (font 2, palette 3) at x + 0x10 / x − 8, y + 8, drawn on top.
- Mode handler `func_80015FCC` (modes 14 and 15), sub-state `D_800C34AA` (`jtbl_80010118`):
  0 init, 1 play, 2 set result `func_8003D184`, 3 pause `func_8002E59C`, 4 match end
  `func_8002D114`, 12 re-pick `func_8002B8EC`. The set result, mode 15's rematch and the re-pick
  all go on by writing sub-state 0, so the next set's init runs a frame later (`Sub::Init`).
  The init (here and in mode 13) leaves the player struct's `+0x16` (flip delay), `+0x18` (move
  delay) and the soft-drop points `D_80069524[p]` as they were (nothing but the drop and
  `func_8003A82C` writes those): `Game::carry_from`. The same RAM crosses modes 13 → 15 → 13
  (`Course::carry`; frame diffed: `challenger` and `cpupause` dumps start from random values).
- Mode 15 (`SetMatch::challenger`, web `CpuMatch::challenge` / `Match::course`; screen example
  modes `challenge` / `challengeback`): entered from mode 13 sub-state 10 (`0x80016968`) with
  `D_80102740/2` = 3, `D_800FFCF8/A` = 0, wins 0, stage `D_800662F0` = 0 (so the SD images are
  VAMIGO13/23/33), `D_800C33F8` = the course stage, `D_80100688` = −1, `D_80103692` = 3. The
  same handler; art (`func_80017B20`): background ST27A (file 0x56) / ST28A (0x57) and page 18
  RANNY1_A (0x59) / RANNY2_A (0x5A) for menu pad 0 / 1, in place of ST18A / TAISEN_A.
  Match end `func_8002D114` (state `D_800B1420`, `jtbl_80011DCC`; timer `D_800C3530`): 0 →
  `func_8002D91C`, 1 (6 in mode 2); 1 waits `D_800B1A28`, then SE 0x1C, script 0x118, 3;
  3 the countdown (script + 0x101 every 120 frames or on the loser's ✕; Start → SE 0x1E, 4;
  0x122 → 5); 4 after 0x3C frames: wins `D_8010ADFC/E` = 0, `D_80100688` = stage; mode 13 alone
  adds 1 to the menu pad's score `D_80069658/C` and continues (above), mode 15 sets the menu pad to
  the winner `D_800B1238` (`D_80100688` = −1 if it changed), the loser's tally
  `D_800699C4[loser]` = 0 and sub-state 0 (a rematch, no re-pick), mode 14 sub-state 12.
  5 after 0xC8 frames: `D_800B137C` = 1 → `D_800B137C` = 0, mode 13 sub-state 0 on stage
  `D_800C33F8` with `D_80100688` = stage − 1 (the stage art reloads), menu pad = winner,
  `D_800B0050` = loser, the loser's tally 0; otherwise mode 0. 6 (mode 2) after `D_800B1A28`:
  sub-state 0 with wins and both tallies 0. 4 and 5 call `func_80040A7C(1, 2)` (the music
  stops) on the frame they leave, in modes 13–15 alike (`Request::Stop` with the outcome).
  The scores are never cleared by modes 13–15 (only Arcade's entry `func_8003A7DC`), so both
  pads' scores carry through the match and back to the course.
- Init: `func_80039DE8` takes each player's level from `D_800120D4[D_80102740[p]]`, a row of
  (colours, starting garbage `D_800C34E4`, quota, speed index):
  1 (4, 0, 0x20, 3), 2 (4, 0, 0x20, 5), 3 (5, 0, 0x20, 7), 4 (5, 0x12, 0x20, 9),
  5 (5, 0x1E, 0x10, 0xC). `func_8001FCA0` puts garbage / 6 into pending[1–6], and the bomb stock
  `D_80100674[p] = D_800FFCF8[p]`. The speed-up is the quota one (`func_80039AC4`) and the
  garbage divisor runs two steps ahead (mode 14 only: `func_8003AE70` tests `D_80100684` = 14, so
  mode 15 uses the plain one). HUD init `func_8003B958`: indicator column
  `D_800C3132` = 1 (special column 2), script 0. "Ready Go" is script 0.
- Bomb: in `func_8001F194`, before the fall, □ (`D_800C34F8[p] & 0x80`) with stock left sets
  player state 12 and `D_800C34E8[p] = 1`. After both cell updates, `func_80038A08` →
  `func_80038C0C` starts the three cut-in scripts with entry 15 (see "Cut-ins" below),
  SE 0xA9, `D_800C34E8 = 2`. The scripts' end op sets 0xFF after 44 frames. State 12 (`func_8001EAB0`) then runs `func_800201C4`: every non-empty cell with a
  colour ≠ 0 in rows H−2 → 0, columns 1–6, gets excited (+0x2A = −1). Then stock −1, state 7,
  `D_800C34E8 = 0`. The pair in hand is dropped. From the press to state 7 takes 46 frames.
- Cut-ins (`func_80038A08`, implemented: `Game::cut_in`, `SdBank::cut_in`, `Screen::cut_ins`;
  checked by `stackers-web/tests/cut_ins.rs`). Called once per play frame by modes 4, 7, 11,
  13, 14/15 (each mode's init clears it through `func_800393FC`; no other sub-state calls it).
  Per player, `D_800C3040` / `D_800C3070` is the matcher's list (`func_80033A14`: group count,
  then sizes) and `+0x28` (`D_800C3068` / `D_800C3098`) the chain value. `func_80038C0C`: a
  non-empty list (consumed) with chain ≥ 2 starts entry min(chain, 14); with no list, a bomb
  flag of 1 starts entry 15, SE 0xA9, flag 2. Three objects per player (`D_8005F670/698/6C0`,
  player 2 `…684/6AC/6D4`, 0x14 bytes: +0 active, +8 frame, +0xC frames left, +0x10 script)
  run SD_CHR tables `0x801A8240/8280/82C0[entry]`: `(u32 frames, u32 frame_ptr)` pairs,
  `0xFFFF` + op (`func_80039300` at start / `jtbl_80011FA0` when stepping: 0 jump, 1 end, 2 set
  +2, 8 flag 0xFF); the data only uses op 1. The step counts the frame down and loads the next
  pair at 0, so each pair shows for its count and the end op runs `Σ frames` after the start
  (44 for every entry except 5's text (48) and 14's text (58) / backdrop (0, empty)). The end
  op (`func_80039658`) sets the player's bomb flag to 0xFF if it is non-zero. Draws
  (`func_80039708`…`func_80039960`, `func_8001B148`, opaque): text at (0x32, 0x28), backdrop
  (0x26, 0x78) at scale 0xA0, burst (0x30, 0x1E) at scale 0x190; player 2 at x 0xDE / 0xD2 /
  0xDE. Layer 2 (behind everything, since it is added last), or layer 0 (in front of
  everything) while that player's bomb flag is non-zero. Art: RENSA2 = page 29 ("n CHAINS",
  "AMAZING" for 14, "BOMB"), RENSA1 = 28 (bursts, palette flashes for chain 5). Solo (mode 4)
  runs it too, but solo doesn't load RENSA1/2, so what it shows there is unknown; only modes
  11 and 14 draw them here.
- The strip over the fields (`func_8003BCD0`, implemented: `stackers_core::roulette`,
  `hud::strip`, `Screen::vs_strip`; checked by `stackers-web/tests/roulette.rs`). Used by modes
  2, 13 and 14/15, called in every sub-state; the scripts only step in sub-state 1:
  `func_8003C340` (both players), `func_8003C3D8`, `func_8003C5A0`, then per player
  `func_8003BD78` (indicator), `func_8003C014` (sparkle), `func_8003C0E0` (message) and
  `func_8003C1AC` × 6 (digits). Drawn by `func_8003C7C4`, layer 0, *before* the cut-ins, the SD
  VM and the HUD (so on top of them). Art: SLOT/RULET.TIM on page 21 and VS/TEAFONT.TIM on page 25,
  loaded by mode 9 (`func_80017DDC`: files 4, 0x53, 0x52 → 21, 25, 17) before every mode (Time
  Trial then puts OJAWAKU2 in 21).
  - HUD scripts (`func_8003B958`, from `BLT_ANM` slots): `+0x3C` digits, `+0x44` indicator,
    `+0x48` sparkle, `+0x4C` message. Each player struct is (script, frame, count `+8`, timer
    `+A`, flag `+C`). Count 0 reads on: ops run inline, and the next pair loads and shows for
    its count (the load frame counts). `0xFF` = stopped. Op 0 jump, 1 end (flag −1: no further
    pair loads, so the ops are re-read every frame).
  - Indicator `D_800C3110 + 0x30·p` (ops `jtbl_80012024`: 0 jump, 1 end, 2 palette
    `D_800C3134` = next word, 3 column +1 (wraps 6 → 0; x `D_800C3136` = `D_800557C4[p][col]`,
    sparkle offset 0, SE 0x19, the sparkle script restarts from `D_801005B0` without resetting
    its count), 4 sparkle offset `D_801003D4[p]` = `D_800557F4[D_800C313A++]` (the 0x80 marker
    restarts at index 1 with value [0])). Script 0: palette 4, 1 frame, op 4, loop (the sparkle
    bobs ±2 px). Script 1: per step palette 2/3/4/5, op 3, 5 frames: so the column steps on
    the start frame and every **5** frames. Script 2: palettes 4,5,2,3 × 4, 2 frames each
    (**32** frames), then jumps into script 0 (`D_800C313C` stays 2). Init: column 1, palette 4,
    x = entry 1, y = 0, script 0.
  - `func_8003C340`: chain value 1 → 2 starts indicator 1, sparkle 1 (one still frame) and
    message 1. `func_8003C3D8`: while `D_800C313C` = 1, player state 4, ○ / ✕ or the set's end
    (`D_80069694`) start indicator 2, sparkle 0 (a looping 99-frame animation, DERO6) and
    message 0 (empty).
  - Digits (`D_801036B0 + 0x90·p + 0x18·c`): `func_8003C5A0` compares incoming + pending per
    column with last frame's (`D_800C2FE8`, not reset per set). On a change, entry 30 + old
    (old clamped to 29) rolls the old digit out: eight 1-frame pairs, then op 5 (entry 30 is op
    5 at once). Op 5 (`func_8003C698`) starts entry `min(current, 29)` in the same frame: 15
    1-frame pairs revealing the digit downwards, then held, palette 8 (0–2), 7 (3–5), 6 (6+),
    set only there. Entry 28 has one pair less (data). Entry 0 is empty.
  - Draws (`func_8001C03C(frame, x, y, mirror, sx, sy, layer, clut, brightness)`: a POLY_FT4
    centred on screen centre + (x, y), `x0 = x − (w·(sx+7)/128)/2`, width `w·sx>>7`; it ignores
    the frame's dx/dy), in the order added: message (`func_8001B4D0` at (0x18 / 0xC8, 0x1E),
    palette 1, only in sub-states 1 and 3); sparkle at (X + wobble − 0x90, −0x60), scale 0x56
    both ways, palette 6; the arrow record `D_800C3120` (RULET (0, 0) 32×40) at (X − 0x90,
    −0x5D), sy 0x56, palette `D_800C3134`; the bar `D_800684E8` (RULET (0x20, 0) 112×32) at
    (−0x55 / 0x5B, −0x61), sy 0x56, palette 1; the digits (`func_8001B4D0`) at
    (0x18 / 0xC8 + 16·c, 0xF); the slots `D_8010AC60` (RULET (0, 0x48) 16×24) at
    (−0x80 / 0x30 + 16·c, −0x61), sy 0x56, palette 6. `D_800557C4` for player 2 is
    192, 208, 224, 240, **252, 268** (the last two 4 px short of the slots).
  - `func_8003F640` (after the draw) only does something in mode 13.
- Pause: Start (alone) on either pad while the clock runs (pad 1 checked first). `D_8010ADAE` =
  that pad, and only its edges drive the menu. Then (`0x800161FC`) a connection change on port 1,
  else port 2, pauses too and makes that port's pad `D_8010ADAE`, overriding the Start check's. Entry `func_8002E514`: state, timer and cursor 0,
  Continue palette 5, Quit 4. Sub-state 3 runs `func_8002E59C` → logic `func_8002E79C` (verified
  by frame diff, `Pause::step_versus`), Continue / Quit only and **not** the solo menu's numbering:
  - 0 select: Start → sub-state 1 at once. ↑ / ↓ move the byte cursor (−1 → 1, 2 → 0); the
    highlighted item gets palette 5 / brightness 0x80, the other 4 / 0x3C. ✕ → SE 0xA7, timer 0,
    the item palette 9, Continue → state 3, Quit → state 2.
  - 1 leaving: after 0x3D frames `func_80040A7C(1, 2)` (music off), mode 0 / sub-state 0. Then
    the "Quit?" buttons still apply: ✕ restarts the wait, △ goes back to state 0.
  - 2 "Quit?": ✕ → 1, △ → 0 (timer 0 either way).
  - 3 continue: sub-state 1, `D_800C3514` = 0.

  The handler then calls the strip `func_8003BCD0`, whose logic only runs in sub-state 1: on
  the frames the menu itself resumed (Start in state 0, or Continue) the indicators and digits
  step once more (a spin can advance and play SE 0x19). After that, Start alone on **either**
  pad (mode 13: the menu pad only) clears both edge words and writes sub-state 1 from any state,
  "Quit?" and leaving included (on the leaving frame mode 0 still wins). `Pause::resume`.
- Pause drawing (`func_8002E59C`, records of `D_8005666C`, scale 128, layer 0): states 1–2:
  13 ("Quit Game? ✕Yes △No", LEVEL2) at (120, 96), palette 5 (`D_8010365E`); otherwise 11 / 12
  (Continue / Quit, LEVEL2) at (120, 96 / 112) with the item palettes and brightness, and 20
  ("✕ Confirm") at (226, 212). Then always 14 ("Paused" panel with its monitor, LEVEL2) at
  (112, 80), palette 8, and 17/18/19 at (50/138/210, 180), palette 6. Sub-state 3 adds it first,
  so it is in front of the strip, the mascot and the HUD (`hud::VS_PAUSE_*`).
- What else sub-state 3 runs (modes 13–15): the fields and queues are drawn (`func_80023294`),
  the mascot `func_8003121C` (mood still updated, the actor drawn but not stepped:
  `func_80031910` skips `func_80032180`/`func_80032590` when `D_800C34AA` = 3), the strip and
  the HUD. Neither the SD VM `func_8002EB38` nor the cut-ins `func_80038A08` run, so SD objects
  and cut-ins are not drawn while paused and are frozen until play resumes (`Screen::set_match`
  / `Screen::vs_cpu`). Solo, Time Trial and Chain Reaction only start SD scripts at game over,
  which can't overlap the pause.
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
- Re-pick (sub 12, `func_8002B8EC`; implemented: `SetMatch::step_repick`, `Screen::repick`,
  test `the_repick_slides_in_picks_and_flies_out_on_the_original_frames`). It is the twin of
  Time Trial's in-game list `func_8002C16C`, for two players. State `D_800699C0`, timer
  `D_800B19A4` (+1 at the top of every call):
  - 0: script 0x142, both cursors `D_800B1400[p]` = 2, list palettes `D_80103658` slots 0–4 =
    0 and 5–10 = 6, arrows (0x28, 0x83) and (0xD8, 0x83) on frame 0 (`D_800B1338`/`D_800B135C`),
    states `D_800B1340`/`D_800B1364` = 0, stocks `D_800FFCF8` = 0, timer 0. Returns before the
    VM (`func_8002EAD4`) and draws nothing of its own.
  - 1: at timer 0x14 → state 2, timer 0. VM only (no arrows).
  - 2: `func_8001D778(0x40, 0xF0, 0x30)` with `D_80100674/6` = the stocks (as on the Arcade
    list). A ready player's △ (state 3, the pad word = 0x10) → state 0, arrow frame 0, and its
    item palette `D_80103658[cursor]` back to 0 unless the other player is not choosing and
    on the same item; the press is consumed. Then per player (`jtbl_80010648` on the cursor):
    state 0: ↑/↓ wrap over 5 (SE 0xA7 always), →/← stock within 0–3 (SE 0xA7 on a change),
    ✕ or Start → 1. State 1 → 2: SE 0xA8, palette[cursor] = 1, `D_80102740[p]` = cursor + 1.
    State 2: arrow frame +1; at 0x1B → frame 0, state 3. Both in state 3 (checked inside the
    loop) → state 3 of the screen, timer 0, script 0x143. Each arrow: y = `D_80100368[cursor]`,
    `func_8001B860(D_80056A88 + 0x12·frame, x, y, mirror = p, 0x80, layer 0, clut 3, 0x80)`
    (LEVEL2, page 17: file 0x52, loaded by mode 9). No △-back to the match from here.
  - 3: △ on either pad → sub-state 12 again (state 0, script id 0). Arrows drawn still while
    the timer < 0x15, after that player 1's x −0x10 / player 2's x +0x10, both y −1 per frame.
    At 0x50 → sub-state 0 (wins reset again).
  - Script 0x42: list items 0–4 (LEVEL2 page 17, 128×24, y 64 + 32·i) slide from x 308 to 96 in
    20 frames; slot 5 is the MSG2 "✕ CONFIRM" caption at (226, 212). Script 0x43 holds for 44
    frames, then slides them off to the right. Both draw with the list palettes (draw mode 1);
    the arrows, added before the VM, are on top of them.

## Chain Reaction (verified, mode 7)

Core rules ported (`Rules::ChainReaction`, `Game::chain_reaction`, `stackers_core::camera`);
screen in `stackers_formats::chain` + `Screen::chain` (`examples/screen.rs` modes `chain`,
`chainzoom`, `chaindrop`); the menu's Chain Reaction launches it (mode 9 → 7). Handler `func_80015AAC`, sub-states `jtbl_800100D8`:
0 init, 1 play, 3 pause, 4 end (`func_8002D628`, entered by `func_8002DC58` with
`D_800B1238` = 1: never a new record).

- End (verified by `tools/framediff.py chainend`): the burst and countdown are mode 4's (see
  "Game over"), but `func_8002D91C` (mode 7) requests **no result script**: it calls SE 0xA9
  four times. Phase 0 then sets the wait `D_800B1A28` = 0x78 (120 frames, not 300) and calls
  `func_80040AE8(1, 2)` (CD state 0x10C = CdlPause: the music stops; `GameOver::music_stop`).
  Phase 1 skips the SD VM in mode 7, so the countdown's 0x118 request is only consumed on the
  next frame (a ✕ there makes it 0x219). Continue → sub-state 0 (a new Chain Reaction).
  Running out the countdown gives mode 4's Game Over (phase 5: XA 7, 200 frames, title):
  4 games that reach it (picked from 80 with `FRAME_SEED=2468`, ≈ 49,600 frames) match to
  mode 0.
- Pause: entry as in mode 4 (Start alone or a port's connection changing, one combined
  check); sub-state 3 runs `func_8002DE14` like mode 4 (same menu). Start on Quit's leaving
  frame also sets sub-state 1 after mode 0 is written; the title ignores it.

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
    and motion ops as SD_CHR (`jtbl_80011EEC`, no op 7; op 4 "set position" is
    `func_800324B4`). A move (op 1, SD_CHR also 7) over n = 0 frames calls that same helper: for
    an actor it is a jump to (x, y) and the script runs on from the op after it; SD_CHR's
    handlers (`func_8002F280` / `func_8002F38C`) only step their own pointer past n (the x word
    is read as the next op), while the helper moves the actor VM's last object (`D_8005F3E0`)
    with the actor's pointer `D_8005F3DC` — a cross-VM slip. No script on the disc has n = 0
    (all 256 SD_CHR ids and every actor mood were run); the port keeps the actor jump and the
    SD_CHR pointer step (`sdchr::motion_op`). Animation commands `jtbl_80011F0C`:
    0 jump, 1 set the actor's done flag `D_8005F3D4`/`D_8005F3D8` and stall, 2 layer, 3/6 hang,
    4 scale tween (`func_8003280C`: unlike SD_CHR's 20.12 tween, whole numbers: step =
    (target − scale) / n truncated, added for n frames, so it can stop short), 5 SE, 7 upload (`func_80032BCC`: 0/1/2 → `0x80180000`/`0x80189000`/
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
    `D_80109638` (see "Title and mode menu"); presets skip it. These `rand()` calls are the piece RNG, so
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
- Divisor `func_8003AE70`: `D_80057680[min(D_80066AD0 / 900 (+2 in mode 14, not 15), 32)]`: 70 for the
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
     if it is settled (state 2) the rest of the column goes back to the **pool** `[0]`
     (`0x80023B48`), so it is spread over all six columns again next frame. Otherwise
     `func_80022908(c, 5 − k, 1, colour)` with pending state 1 (falls in). Colour 6 if
     c − 1 == `D_800C3132[p·0x18]` or the mode is 11, else 0 (grey). Placed counts go to
     `D_80100640[p]`, `D_801005FA[p]` and `D_800B001C[p]` (the grey-landing sound gate).
- Garbage shake `func_8001A8B8` (implemented: `versus::Shake`, `Versus::step_shake`, table
  `tables::SHAKE`, checked by `garbage_tables.rs`). Every landing blob (`func_8002193C`, colour
  and destination aside) sets `D_80100408` = its player's `D_80100640[p]`, so 0 once that player
  has no drop pending. The two-player frame draws call it every frame they run: `func_8001A82C`
  (mode 2 sub-states 1/3/4, modes 13 1/3/4/10, modes 14/15 1/3/4/12; after the mascot
  `func_8003121C` and before the end check) and `func_8001A70C` (mode 11, first thing in
  sub-states 1/3/4, after the list in 12). A non-zero `D_80100408` starts a shake: the level
  `D_800699CC` is picked with three `if`s and no `else` (< 4 → 0, < 8 → 1, < 17 → 2, else SE
  0x1F and 3), so only levels 2 and 3 happen; length `D_800B1376` = `D_80056EC0[level][0]` (41
  i16 per level), step `D_800B0018` = 1, count `D_800C2F68` = 0, and `D_80100408`,
  **`D_80100640` and `D_80100642` are zeroed**. Then (that frame too) while count < length:
  count += 1, y = `table[level][step++]`; else y = 0 and length = count = 0. Both frame halves
  (and W_ZERO in mode 11) are drawn at (x, y): level 2 is 27 frames of −8 … 7, level 3 38 frames
  of −12 … 7. SE 0x1F is why the grey-landing sound is muted for 17+ blob drops. The words are
  globals, so a shake carries over a set or stage init. (An earlier harness stubbed both frame
  draws, hiding this.)
- `D_800C3132`: the special-column indicator of the two-player HUD (struct `D_800C3110`, stride
  0x30). Init 1 (`func_8003B958`); stepped by the HUD script in `func_8003BD78` (op 3: +1, wraps
  6 → 0, SE 0x19, the arrow's x from `D_800557C4`). Ported for mode 14 as
  `roulette::Roulette`, which feeds `Game::special_column`.
- All clear (`func_8003AA78`, end of the score step `func_8003A82C` in modes 11, 13, 14, 15;
  implemented: `Game::check_all_clear` / `step_all_clear`, `Screen::all_clears`; checked by
  `stackers-web/tests/all_clear.rs`). The flag `D_800B1234[p]` is set by **every** clearing
  step (`func_8003AF24`, also for a single clear) and only cleared by an all clear or the mode
  inits (`func_8003AEE4/AF04`). If a player's empty-cell count (histogram `D_800662B8`) is 0x6C
  (the whole 6×18 area) with the flag set: `func_8003AC6C(p)` starts the sprite, this turn's sent
  blobs (`D_801005EE`/`D_801005F4`) go to the opponent's pool again (and are zeroed), the
  all-clear count `D_800B138C[p]` += 1 and score += `D_800576C8[min(count, 7)]` after the
  increment (so 25,000, 60,000, 100,000, 160,000, 300,000, 770,000, then 1,200,000). No sound.
  The count is cleared with the score by `func_8003A7DC` (modes 4, 5, 11 inits), so it carries
  across vs CPU stages, sets and mode 15 like the score.
  - Sprite: objects `D_800C2FC0[p]` (stride 0x10: +0 script, +4 frame, +8 timer, +0xA count,
    +0xC −1 = stopped), script `BLT_ANM` table `+0x40` entry 0 (entry 1 in mode 11), stepped by
    `func_80024C14` in `func_8003ACC0` (skipped once stopped). 30 × 3 frames cycling cluts 3/4/5
    of RULET (u 0, v 120; mode 11: cluts 1/2/3 of OJAWAKU2, u 112, v 0; same palettes), 96×96,
    "All Clear! Obstacle Blocks ×2"; the step that reads "done" still draws, so 91 frames.
    `func_8003AD34` draws it with `func_8001B148` at the field's camera and scale:
    (8 / 0xB8 + 0x10, 19·16 − camera − 240) = (24 / 200, 80), layer 1. It is queued before the
    two-player frame (`func_8001A82C`), so in front of it; in mode 11 after W_ZERO
    (`func_8001A70C` runs first), so behind it.
- Pieces (`func_80022F14`): when both colour counts `D_800C3534[2]` match, pairs come from the
  shared ring `D_80100318[32]` (write head `D_8006626E`, read heads `D_800B0054[p]`): a player that
  has read everything rolls two colours into the ring first (satellite then pivot). Player 1's
  initial queue is player 0's (`func_80023474`, no `rand()`). Otherwise `func_8002319C` rolls.

## Special blobs (verified, colour 6)

Colour 6 is special garbage: it comes from garbage dropped into the two-player special column
(`D_800C3132`) and from all garbage in mode 11, and a set winner's field / a Time Trial record
turn blobs into it for the pop.

- **It pops on landing, except in mode 11** (found by the two-player `tools/framediff.py`):
  `func_8002193C` places a landing colour-6 blob in state **13** (with SE 0x1E) when
  `D_80100684` ≠ 11, and in state 2 only in mode 11. State 13's cell handler `func_80021730`
  empties the cell when its 48-frame script ends: nothing above falls, and **no count is
  decremented**, so each popped special blob stays in the grey count `D_801005FA[p]` for
  the rest of the set. Falling blobs collide with state 13 as with state 2. So in the set
  match and vs CPU, garbage aimed at the special column just vanishes. Ported:
  `Field::special_settles` (set for `Rules::TimeTrial`), `Field::special_pops`, `se::POP`;
  `field::tests::special_blobs_pop_on_landing_except_in_mode_11`.
- In mode 11 (and wherever a colour-6 blob is settled) it behaves like grey except that it
  matches with itself:

- It **does match**: neither the match check `func_80033A14` nor the flood fill `func_80033DFC`
  tests for colour 6, and landing sets `just landed` (`+0x16`) for every colour. So 4+ connected
  colour-6 blobs clear once one of them has landed (e.g. 4–5 garbage blobs dropped into the
  special column), counting as a normal clear for the chain, the attack and SE 9+.
  Its state-4 script is a bare done marker, so the group vanishes on its first cell step; the
  clear comes off the grey count (`D_801005FA`) and does not excite neighbours.
- Arms don't cross it: the span scan `func_80024CE0` only continues over cells whose state is 0
  or colour is 0, so colour 6 ends an arm like a coloured blob.
- It can't be poked or burst: `func_8002602C` (poke) and `func_80034A7C` (burst under a cleared
  arm) test the cell with one `lw` of state + colour (`+0`/`+2` together) `== 2`, i.e. settled
  **and** colour 0.
- It never reaches out: the settled handler `func_80020A24` clears `excited` on colour 0 or 6
  instead of starting arms; the neighbour mask `func_800220A0` skips 0 and 6.
- State 13 is also the end-of-round pop: `func_8003B54C` (Time Trial record) / the set winner's
  `func_8003B7E0` set it, `func_8003B644` steps the pop and empties the cell, taking 1 off player
  1's grey count. State 3 is a no-op in the cell table (like 0 and 6). The colour-6 scripts for
  states 7, 11 and 12 exist in `BLT_ANM` but are unreachable in play.

The port already behaves this way (`Field` checks `color == 0` where the original's word compare
implies it); `field::tests::special_blobs_match_each_other` pins the matching.

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
  state 0, whose handler `func_8001E270` (mode 11, p = 1) sends it straight back to 4 (found by
  `tools/framediff.py trial`). P1's queue isn't drawn (`func_80022A5C`).
- All garbage is colour 6 in mode 11 (see "Garbage"); p1 never runs a match check (it never
  spawns), so its field just fills.
- Clock `D_80100648` (`func_8003D8D8`): in mode 11 a minute overflow sets it to `0x3B3B3B`
  (stops at 59'59"59).
- Pause entry as in mode 4 (Start alone or a port's connection changing while the clock runs). Sub-state 3 draws the frame, the
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
  `func_8001A8B8` (see "Garbage") shakes W_ZERO when player 1's target field lands a drop.
  Queue patch `D_80055F14` comes from ST53B.
- Clock HUD `func_8003D9B0(clock, best)`: " ", minutes (2, zeros), "kS", seconds, "kD", then
  frames / 6 and (frames % 6)·2 capped at 9 (1 digit each), font 1; clock at (0xD0, 0xB4)
  palette 7, best at (0xD0, 0xCC) palette 1.
- Mascot `func_800312AC` (implemented: `mascot::Kind::TimeTrial`, `TimeTrial::mascot`, web
  `Trial`, test `time_trial_mascot_scripts`): sub-states 0/1/3/4 (not the course list).
  Sub-state 0 sets `D_800699D0` = 0xFF, `D_8010AE00` = 0, so the call runs `func_8003151C`
  (table `D_801A8300`, mood 0, `D_8005F640/F648` = 0; `D_8005F65C` is kept). Later calls:
  unless paused, `func_8003132C` (the test reads the sub-state after the pause menu ran, so the
  frame the menu resumes steps it, with that frame's pad: `TimeTrial` steps the mascot there);
  then `func_80031910(0)`, drawn layer 0 in front of the clock
  (added before `func_8003D9B0`) and behind the pause menu and result scripts.
  - `func_8003132C`: mood 7 (the start signal, VNAO3) for the first 0xF0 calls; then 0 while the
    clock `D_800667A4` is stopped, 4 while player 1's chain `D_800C3068` ≥ 2, else by pad 1's
    held word `D_80100240`: → 3 (SE 0x75), ← 2 (0x74), ↑ 5 (0x73, shrinks to 0x50), ↓ 6
    (grows to 0xAA), nothing held 1 (and `D_8005F65C` = 0). A direction's SE plays only while
    `D_8005F65C` is 0 and sets it, so one line per hold (bank `DERO_4`, program 2, keys
    60–62). Its pause `rand()` never runs (not called in sub-state 3), so no RNG use. SEs are
    dropped once `D_80069694` ≠ 0. Moods 0–6 upload VNAO1 as page 26.
- Music: index 9 (Track 11). `func_80038A08` (state `D_800C30B2`) is the cut-in machine, ported
  (see "Set match" → Cut-ins). Mode 9's voice is a kind-3 call, which only preloads the bank.

## Sound effects (verified)

Implemented in `stackers-formats::{vab, sound}`, `stackers-core::se` and `stackers-web::sfx`.

- `func_80040740(id, kind)` dispatches on kind (jump table `0x80013AC4`); kind 1 =
  `func_800407C4`, a one-shot effect: entry `D_80013160[id]` = (bank slot, program, key, –),
  played with `SsVoKeyOn((vab << 8) | program, key << 8, D_80100388, D_80100358)`.
- Bank slots `D_80012A94` (0x44 bytes): 8-byte name + `u32` kind (0 `VAB_SE`, 1 `VAB_CHR`,
  2 XA). Slots 0–23 are the 24 VABs on the disc; slot 0 = `DEROMAIN`. Set up by `func_80040394`.
- VABs are standard v6 (`pBAV`); every tone covers a single key, so an effect is one sample.
- `SsVoKeyOn` (`func_8004D578` → `func_8004E63C` → `func_8004DFA0`, SE channel 0x21) takes a
  voice from `func_8004E764`: a free one, else the lowest-priority / oldest. Nothing checks for
  the same effect already sounding, so repeated requests overlap on hardware too (the page's
  one buffer source per request is right). Only effect 0xB8 (`OUTGAME`) has a looping sample.
  15 table entries (102, 160, 170–182, all `OUTGAME`) name keys their bank doesn't define.

In-game effects (all `DEROMAIN`):

| id | when | source |
|---|---|---|
| 2 | rotate, kick or flip attempt (also when the flip is on cooldown) | `func_8001EBF0` |
| 7 | sideways move that succeeded | `func_8001EF9C` |
| 3 | pair touches down (→ player state 3) | `func_8001F5EC` |
| 4 | an arm started reaching this frame (`D_800FFDBC`) | `func_800248F4` |
| 8 | an arm poked a grey blob | `func_8002602C` |
| 3 | a coloured blob landed this frame in a slot that was empty (flag `D_8010067C`, once per frame) | `func_8002193C`, `func_80021C68` |
| 100 | grey landed this frame (flag `D_80101ED0`: colour 0, or colour 6 in mode 11), while `D_800B001C[p]` < 17 | same |
| 9 + n − 1, max 17 | clear number *n* of a chain (counter `D_800B1380`) | `func_8002C770` |
| 5 | 24 frames after the latest match (`D_80066D3C[p]` read as 16) | `func_80026E8C` |
| 6 | 39 frames after it (read as 1); a new match restarts the count, so only the last one ends | `func_80026E8C` |

Sound 100's gate `D_800B001C[p]` (`Game::grey_gate`, `se::GREY_LAND`; DEROMAIN program 2,
key 62): the garbage drop (`func_800239A0`, player state 4) sets it to the blobs it placed
(`D_80100640[p]`), spawn (`func_8001FFD8`) zeroes it, and in modes 14/15 `func_8001FCA0` sets
it to the set's starting garbage `D_800C34E4[p]` after its spawn call. So a drop of 17+ blobs
lands silently; greys falling after a clear (gate 0) and smaller drops thud. In
`func_8002193C` a landing sets neither flag if the destination cell's state is non-zero
(read before the blob moves, so a blob that comes to rest in its own slot is silent: a locked
pair's halves play nothing after the touchdown sound), and
colour 6 outside mode 11 sets neither; any colour other than 0 and 6 sets `D_8010067C`.
Checked by `game::tests::grey_landing_sound_is_gated_by_the_drop_size`.

`D_80066D3C[p]` (halfword per player) is set to 0x28 by `func_80034C3C`, which the flood fill
`func_80033DFC` calls for every cell of a group of 4+ (the player-update match check), and
zeroed by the field init `func_80026F14`/`func_80026F50`. `func_80026E8C` runs right after each
player's span step (`func_80024878` both players, `func_800248C4` player 0 only): it plays 6 if
the value is 1 or 5 if it is 0x10, then decrements it when non-zero. Since the match frame's own
span step already reads 0x28, the sounds come 24 and 39 frames after the match. Ported as
`Game::clear_timer` / `step_clear_timer` (`se::CLEAR_LATE`, `se::CLEAR_END`).

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

## XA music (verified)

Implemented in `stackers_formats::xa` (table, decoder; `examples/xa2wav.rs` dumps every entry),
`stackers-web::Xa` and `web/play.html` (caches the three raw files, decodes on first use).

- `func_80040A7C(index, kind)`: kind 0 only stores the index (unused), kind 1 sets CD state −2
  (CdlPause, sync, then play), kind 2 state 0xC (CdlPause: stops CD-DA as well as XA).
- Table `0x80013468`, 10 entries of 12 bytes: `u8 file` (`DERODERO/SOUND/XA/SND_XA0n.STR`),
  `u8 channel`, 2 pad, `i32 sectors`, `i32 flag` (0 loop, −1 once). States 1–11: file position
  from the file table `D_80100670` (36-byte entries), end = start + sectors (`D_800B0044`);
  setmode 0x48 (XA + filter, single speed), setfilter file 1 / channel, setloc, ReadS, then GetlocP
  until the end: flag −1 → pause (0xC), else seek back (state 5).
- Files: every audio sector is stereo 37.8 kHz 4-bit (coding 0x01), submode 0x64 (0xE4 = EOF),
  channel *c* in every 4th sector from *c*, null filler sectors (submode 0) elsewhere. At single
  speed a file sector is 504 sample frames, so an entry lasts `sectors / 75` s.
- Entries: 0, 1, 3 (`SND_XA00` ch 0–2, ~50 s loops) and 2 (`SND_XA01` ch 0) have no callers in
  this exe; 4 good-end jingle, 5 bad-end jingle, 6 "challenger" (`func_80016480`), 7 "Game
  Over" (`func_8002D114`, `func_8002D628`), 8 staff roll, 9 ending scene (`func_80030798`).

## Music director (verified)

Implemented in `stackers_core::bgm` (Director, run by `Session`, `TimeTrial`, `VsCpu`,
`SetMatch`; requests as `music` getters: −1 none, −2 stop, 0–99 CD-DA, 100 + n XA n).

- `func_80039EE4` each frame from `func_80014634`: if `D_800C34B4` is set, `func_8003A050`
  counts `D_800C34B6` to 4 and on the next frame starts `D_800C34B2` (0 → CD-DA `D_800B1280`,
  3 → CD-DA 2 = danger track, else XA). Then by mode (`jtbl_80011FF4`): 4 → track 8 and
  `func_8003A14C`; 11 → 9, `func_8003A14C`; 7 → 10, `func_8003A3C8`; 13 → by level row,
  14 → 6, 15 → 4, all `func_8003A4F4`. `func_8001FCA0` resets the state `D_800C34B0` to 0.
- Each change calls `func_80040A7C(0, 2)` (stop) at once and queues the next start.
- `func_8003A14C`: state 0 → 1 (normal) when sub-state `D_800C34AA` = 1 and `D_80066AD0`
  ≥ 0xB5; 1 → 2 (danger) at blobs ≥ 0x31, 2 → 1 below 0x24. Then the end check: `D_80069694`
  odd → XA 4 if mode 4 and `D_800B1238` = 0 (record) else XA 5; `D_80069694` = 2 (Time Trial
  record) → XA 4; state 3.
- `func_8003A3C8` (mode 7): start at 0x90 frames, danger at ≥ 0xC1, back below 0x90, no end check.
- `func_8003A4F4` (13–15): start at 0x90; danger when either player ≥ 0x31 and level row
  `D_800662F0` < 12, back when both < 0x24. End (`D_80069694` = loser + 1, `func_8002DD28`):
  the menu pad lost → XA 5 in mode 13, XA 4 in 14/15; the other player lost → XA 4.
- Blobs = `D_801005FA[p]` + `D_80100634[p]` (see "Blob counts"; `Game::blobs`).

## Blob counts (verified)

Two i16 counters per player, `D_801005FA[p]` "grey" and `D_80100634[p]` "coloured" (third
`D_80100638[p]` is only ever zeroed). Zeroed by `func_80022718` (both players) and
`func_8002277C` (player 0). In the port: `Field::grey` / `Field::colored`, `Field::blob_count`.
The cell update reaches them through pointers set at its start by `func_800202E8`
(`D_800C3518` → grey, `D_8010035C` → coloured, `D_801003E0` → third).

- Lock `func_8001F67C`: coloured += 2, whatever the colours. The pair under control is in
  neither count.
- Mode-7 □ (`func_800200F0`, after the lock with both colours 0): coloured −= 2, grey += 2.
- Garbage drop (`func_800239A0`): grey += blobs placed per column (colour 6 included).
- Preset `func_80027098` (player 0's counters, hard-coded): value 0 → grey + 1, any other
  colour (6 too) → coloured + 1.
- Clear finished, `func_80020C58` (states 4, 5): colour 0 or 6 → grey − 1, else coloured − 1.
  `func_80021030` (state 7): grey − 1.
- `func_8003B644(p)` (the winner's / record pop): each emptied colour-6 blob takes 1 off
  **player 1's** grey (`D_801005FE`, hard-coded) whichever field popped.
- Pointer quirk (ported): `func_80021730` (state-13 handler inside the cell update, reached
  in play by special blobs that landed, see "Special blobs") moves the *pointers*
  `D_800C3518 −= 2` bytes and `D_800C3500 += 2` instead of the values when a pop ends.
  `func_800202E8` resets them at the start of each cell update, so for the rest of that update
  grey decrements (`func_80020C58` colour 0/6, `func_80021030`) land 2 bytes lower per finished
  pop: from `D_801005FA` (p0) / `D_801005FE` (p1) down through `D_801005F8` (unused),
  `D_801005F6` (the mode-4 speed cap), `D_801005F4` / `D_801005EE` (p1 / p0 blobs sent this
  turn), `D_801005EA` / `D_801005E8` (the CPUs' move delays); p1 after 2 pops hits p0's grey.
  A whole special column lands, and so pops, on one frame, so several pops per update are
  normal. `D_800C3500` has no reader; the other `D_801005FC` readers (`0x80032ACC`,
  `0x80038958`) are unreferenced code. Ported as `Field::misrouted` + `field::grey_slot`,
  applied by `Game::misrouted_grey` / `Versus::misrouted_grey`.
- Readers: the music director and mascots (`lhu` sum), the CPU's plan snapshot
  (`D_80055880 + 6·p`), the erase number. Checked: the sum equals the field's non-empty
  cells (columns 1–6) on every frame of CPU-vs-CPU matches (`cpu::play::blob_counts_follow_the_field`).

## CPU (verified, modes 13 and 2)

Implemented in `stackers_core::cpu` (records in `tables::CPU_PARAMS`), run by
`Versus::cpu`. Checked against the original by running `func_80034C60` in a small MIPS
interpreter (`tools/mips.py`, `tools/cpudiff.py`; 20,000 random situations, no differences).

- Where it runs: the pad read `func_80014F20`, in mode 13 while the sub-state is 1, calls
  `func_80034C60`: the AI drives player 0 if the menu pad `D_800C2FB8` is 1, player 1 if it is 0
  (so always the human's opponent), and its word `D_800C2F58[p]` replaces that player's held pad
  except Start/Select (`& 0x900`). Mode 2 (attract demo) calls `func_80034DFC`, the same for both
  players, with records `D_80055970` / `D_80055994` (variant 1, stages 5 and 8;
  `func_80035088`). The AI sees last frame's state (it runs before the player updates).
- Records `D_8005588C[variant][stage]`, 12 bytes, copied to `D_800B13E8 + 12·p` by
  `func_80034F74` (mode 13/14 init, which also sets mode 1 and clears both words); variant =
  the level table's (`D_800B1398/13A0`: table `0x8001218C` → 0, `0x8001234C` → 1), stage =
  `D_800662F0`. Fields: select (always 2), flags, blob limit, a limit for the always-zero third
  count, drop y, delay. Only variants 0 and 1 hold real data.
- Per frame (`func_80034C60`), by the player state `P+0xC`:
  - 0: `func_80038490`: if flags & 4 and cell (3, 9) is settled, delay `D_801005E8[p]` = 0. If
    the mode is 0 or the delay ≤ 0, act (`func_80038598`), else idle (`func_80038844`). Then the
    delay counts down to 0.
  - 11 or 2: plan. Counts snapshot (`D_80055880 + 6·p` = grey/special `D_801005FA`, coloured
    `D_80100634`, third `D_80100638`, which is only ever zeroed); mode switch; lists; choice;
    word = 0; delay = record delay. Repeated every frame of those states.
  - others: nothing (the word is held).
- Mode `D_800B1428[p]` (`func_80035160`), with n = grey + coloured (`Field::blob_count`): 0 → 1 if n ≤ limit;
  1 → 0 if flags & 1 and the pending garbage `D_801006A8[p][0..6]` (pool and columns 1–5 only)
  sums to ≥ 4, else → 2 if n ≥ limit; 2 → 0 if n ≤ limit.
- Lists (`func_800353A0`): six lists of up to 16 entries (count, then 12-byte entries: x, y of
  the column's top cell, rotation), stride 0xC4: L0 `D_80066608`, L1 `D_80066940`,
  L2 `D_80066480`, L3 `D_80066BB0`, L4 `D_800667B8`, L5 `D_800662F8`. For columns 1–6
  (`func_80035468`), A = next pivot colour (`D_80069A08`), B = satellite (`D_800699F0`):
  - top row = first row from 6 down that is settled or the floor.
  - three(c, r, k) (`func_80035964`): two of (c, r)'s four neighbours are k, or a neighbour that
    is k has another k neighbour. contact(c, r, k) (`func_800356D0`): one of (c, r),
    (c − 1, r − 1), (c + 1, r − 1) is k and three() there.
  - Pivot tier: if (col, top), (col ± 1, top − 1) has A: contact(col, top, A) → tier 0
    (`func_800366A0`) else tier 1 (`func_80036928`); otherwise tier 2 (`func_80036BB0`).
  - Satellite results 0 (forms three), 1 (touches), 2 (neither) for rotation 0 (up,
    `func_80036E38` on row top − 1: A = B, or B at (col − 1, top − 2) or — a bug — at
    (col + 1, col − 1), then contact on B), rotation 3 (left, `func_80036F84`, column col − 1,
    none for col 1) and rotation 1 (right, `func_800371A8`, none for col 6); the side ones look
    at that column's own top cell and its upper neighbours for B.
  - List = `[[0, 1, 2], [1, 3, 4], [2, 4, 5]][tier][result]`. L3–L5 refuse column 3 when its
    top row is < 9 (`func_800376AC` etc.).
- Choice (`func_80037A00`): first non-empty list in the mode's order — mode 0: L0…L5; mode 1:
  L3, L4, L5, L1, L2, L0; mode 2: L1, L2, L0, L3, L4, L5 — picked by select: 0 `rand() % n`,
  1 the last entry if it is higher (smaller y) than the first, else the first; 2 the deepest,
  the later one on a tie. Target `D_800C2F10 + 12·p` (x, y, rotation); unchanged if every list
  is empty.
- Act (`func_80038598`): nothing if the satellite can turn neither way (`func_800332F4(±1)`:
  the cell at the turned offset, `0x800575C0`, is non-empty). Rotation ≠ target: if the word is
  non-zero release (0), else ○ for target 1, ✕ for 3 (0 and 2: nothing). Rotation matches:
  word = 0 / → / ← toward the target x. Then if flags & 2: ↓ when y > drop y and aligned;
  otherwise 0 when aligned.
- Idle (`func_80038844`): `D_8010ADA8[p]` −1; at −1: release if the word is non-zero, else a
  random direction (`rand() % 4`: →, ←, →, ←); reset to 30. The counter is never initialised
  (0 at boot), so it carries over between games.
- The AI's `rand()` calls are the BIOS ones, shared with the piece generator.

## Attract demo (verified, mode 2)

Implemented in `stackers_core::demo` (`Demo`), drawn by `Screen::demo` (web `Demo`; screen
example mode `demo`). The title's state 5 enters it (`Front::launch_demo`).

- Art (title state 5: `func_80017744`, `func_80017DDC`, `func_80018ADC`, `func_80017C40`, then
  RENSA1/2 again): NULL (`0x80138000`), VS_WIN2 → 20, ST14A (file 0x6F) → `0x80138000` (the
  background), ST14B (0x70) → 18, W_AMIGO (0x75) → 24, DEMO/SETUMEI0–3 (0x12–0x15) → 14–17.
  So vs CPU stage 6's art (`hud::DEMO_STAGE`, `hud::demo_page`). File 0x16 (DEMO/SD_LOSE)
  isn't loaded here.
- Handler `func_80018F24`, sub-state `D_800C34AA` (`jtbl_800103C0`):
  - 0 init: `func_80019498` (height 19, queue x 0x88/0xA8, `D_80069694` = 0),
    `func_80039CEC` (any mode but 13: row 3 of the first table, `D_801005F6` = 15),
    `func_8001FC64`, `func_8001DA4C`, `func_8003B958` (strip), camera/zoom words,
    `func_80035088` (AI records `D_80055970` = `D_8005588C[1][5]` for p0, `D_80055994` =
    `[1][8]` for p1, AI mode 1, words 0), panel `D_80100360` = 0, preset `D_80100680` = 0.
    No `D_800C3514` request: no "Ready Go"; no mascot, no HUD (`func_8001DB4C` isn't called),
    no music (the director `func_80039EE4` only handles modes 4–15).
  - 1 play: `D_801003DC` += 1, the two-player frame (`func_8001E020`, `func_800202AC`,
    `func_80024878`, `func_8003A82C`), strip, frame, cut-ins. If (clock `D_800667A4` &&
    `D_801003DC` ≥ 300) or `D_80100360` = 7: SE 0x12, timer 0, request `0x147 + D_80100360`,
    `D_800C2F84` = 400 for panel 6 else 350, sub-state 3. Then `func_8003CBAC` (set end).
    The timer isn't reset by sub-state 0 (title state 5 leaves 4), so the first panel comes
    after 296 frames; the 0x8F-frame intro doesn't count (clock stopped).
  - 3 panel: timer += 1, fields drawn but not stepped, strip (no step), frame,
    `func_8002EBA0` (SD VM, draw mode 4), cut-ins step. At timer = `D_800C2F84`:
    `D_80100360` += 1 (2 → 3), timer 0, sub-state 1. Panels: 0x47 (SETUMEI0 top, clut 0),
    0x48 (SETUMEI0 bottom), 0x4A, 0x4B (SETUMEI2), 0x4C, 0x4D (SETUMEI3); 0x49 (SETUMEI1 top)
    is skipped. Each object grows from scale 0 to 128 and back.
  - 2 / 4: `func_8003D184` / `func_8002D114`. `func_8002D91C` requests nothing in mode 2, and
    phase 0 → 6: after `D_800B1A28` = 300 frames (burst step each frame), timer 0, wins and
    tallies 0, sub-state 0 (the panels start over). Sets = `D_80103692 / 2 + 1`;
    `D_80103692` is 0 in the exe image and only the Arcade set-match menu writes it, so the
    port plays one set (sub-state 2 never reached).
  - Every sub-state, at the end: `D_80100360` = 7 or the merged pad word `D_800C34AC` (32-bit)
    `& 0x08000800` (Start held on either pad) → mode 0.
- AI: the pad read (`func_80014F20`) calls `func_80034DFC` whenever the sub-state is ≠ 0 (so
  in the panel and match end too, with its `rand()` calls), and its words replace both pads but
  `0x900`. `func_80034DFC` is `func_80034C60` for both players.
- Draw mode 4 (`func_80030070` → `func_8001CBE4(frame, x, y, flip, sx, sy, layer 0, frame clut,
  0x80)`): a POLY_FT4 with corners `x + dx·sx/128 − 160`, `y + dy·sy/128 − 120`, size
  `w·(sx+2)/128` × `h·(sy+2)/128`, semi-transparent (`func_80045764(p, 1)`), on an ordering
  table drawn with the screen centre as offset (`sdchr::panel_placement`). Layer 0, added after
  the strip, so under it.

## vs CPU (verified, mode 13)

Implemented in `stackers_core::vs_cpu` (`VsCpu`), drawn by `Screen::vs_cpu` (web `CpuMatch`).

- Course list (mode 5 sub-states 6/7, `func_800290F4` / `func_8002922C`): course
  `D_800C30C8` 0/1/2 with first stage `D_800662F0` and end `D_800C2FE0` = (0, 3), (3, 14),
  (3, 14). The human plays on the menu pad `D_800C2FB8`; the AI (`func_80034C60`) the other.
- Handler `func_80016480`, sub-state `D_800C34AA` (`jtbl_80010150`): 0 init, 1 play, 2 stage won
  `func_8003CF2C`, 3 pause, 4 lost `func_8002D114`, 10 challenger, 11 stage load.
- Init: `func_80039CEC` puts both players on row `stage` of `*(0x8001215C + 16·course)`:
  `0x8001218C` (`LEVELS`) for courses 0/1, `0x8001234C` (`HARD_LEVELS`) for 2. Rows are 4 × i16
  (colours, CPU variant, quota, speed); both tables give a row the same colour count. Standard
  rules, no handicap, bomb stock `D_800FFCF8` (0). `func_80034F74` loads the AI record. Score
  `D_80069658/C` carries over. If `D_80100688` ≠ stage (a new stage), sub-state 11 starts at
  `func_80030394` state 4: two blank frames, then `func_80017EB0` loads the stage art and play
  starts with script `0x100`; otherwise straight to sub-state 1. (States 0–3, an opponent
  presentation with scripts `0x224/0x225 + 2·stage`, are not reached from mode 13.)
- Stage art (`func_80017EB0`, files of `D_80057298`): 1P_WIN (0x54) as page 20; by stage the
  background STxxA (into `0x80138000`), STxxB as page 18 and an opponent `W_*` as page 24:
  0 ST26/AMIGO, 1 ST06/TENSI, 2 ST08/QUEN, 3 ST26/AMIGO, 4 ST02/DENKYU, 5 ST13/ELEPAN,
  6 ST14/AMIGO, 7 ST25/ATOS, 8 ST04/QUEN, 9 ST05/TENSI, 10 ST00/ATOS, 11 ST10/NEKO,
  12 ST11/TAMA, 13 ST12/TAMA.
- Play: as mode 14 (strip, cut-ins), no HUD from `func_8001DB4C` and no stocks; the menu pad's
  score at (0x80, 0xD0) (8 digits, font 1, palette 1, zeros). Start on the menu pad or either
  port's connection changing, with the clock running, pauses (`D_8010ADAE` = menu pad; the
  two-player menu, see "Set match"; only the menu pad's Start resumes). Start on the other pad
  (any time, checked after the pause, so it overrides one entered that frame) → sub-state 10: script
  `0x244`, SE 0x1D, XA 6 (the end check `func_8003CDAC` still runs after it, so a round that
  ends on that frame goes to sub-state 2 or 4 with those three already requested); after the
  menu pad's Start or 0x12D frames, mode 15 (difficulties 3/3,
  3 sets, `D_800B137C` = 1, `D_800C33F8` = stage). Mode 15's match end then returns to mode 13
  on that stage with the winner as the menu pad (see "Set match" → Mode 15).
- End check `func_8003CDAC` (`func_8002DD28`; both out → player 1 wins): `func_8003B2B8` bursts
  the loser. Human won: stage + 1, `D_800B004C` = (stage == `D_800C2FE0`), sub-state 2.
  CPU won: `func_8002D0C4`, sub-state 4 with `D_800B1A28` = 300.
- Stage won (`func_8003CF2C`, timer `D_80066ACC`, phase `D_801026D8`): 0 script `0x240` + pad
  (a `0x2xx` request: 20-byte records with start positions, `func_8002EF44`, puts the 1P_WIN panel
  over the winner's field); 1 tally + burst step until 0x5B; 2 until 0xB5; 3 results shown until
  0xF1; 4 the bonus moves into the score 1000 a frame (SE 0x1B), ✕/Start or 0x12D → 5; 5 the rest
  at once, ✕/Start or 0x169 → 6; 6 course cleared → mode 16 (ending, see "Ending"), else sub-state 0.
  - Chain value `D_800B1388` (`func_8003D2AC`, the winner's): 10 × best chain `D_8010AE02/6` +
    10 × (sum `D_8010AE60/2` / turns `D_8010AE50/2`), as i16. `func_8001FB78` records them at each
    spawn with a non-zero chain `D_800C3068`.
  - Bonus `D_800C3400` (`func_8003D388`) = 100 × (time + chain), time by play frames
    `D_80066AD0`: < 1200 → 1000 + (1200 − t) / 6, < 7200 → 1000, else 0.
  - Rank (`func_8003D448`): first of 120/100/80/60/0 (`D_80057728`) the chain exceeds
    (unsigned): SPECIAL A, A, B, C, D, else LUCKY; text at (x, 0x81), x = 0x20 (player 1 won) /
    0xD0. Time (`func_8003D570`): " m'ss"cc" at y 0xA0 (digits 0xA2, font 1, palette 1). Bonus
    (`func_8003D754`): 9 digits at (x, 0xBA).
- Lost (`func_8002D114`, mode 13): no emblem (`func_8002D91C` sets `D_800C3514` = 0), 300 frames,
  then the continue countdown. Continue: score + 1; on courses 1/2 below stage 13 the end
  becomes 13 (the last opponent is skipped); same stage, no reload.

### vs CPU mascot (verified, mode 13)

Implemented in `stackers_core::mascot` (`Kind::Versus`, `Mascot::versus_step`, run by
`VsCpu::step_mascot`), `sdchr::VS_ACTORS`, `hud::VS_CPU_SHEETS` / `vs_cpu_sheet` and web
`CpuMatch` + `Screen::vs_cpu` (drawn last, in front of the strip); test `versus_moods`.

- `func_8003121C`, run in mode 13 sub-states 0 (after `D_8010AE00` = 0 → init), 1, 2, 3, 4 and
  10 (not 11). Init `func_800319CC`: both actors' tables = `D_801A8200[stage]` (that word is the
  mood list itself, so `actor_script(0x801A8200 + 4·stage, mood)`), moods 0, `D_8005F638` = 0.
  Sub-state 0 enables only the CPU's actor: menu pad `D_800C2FB8` = 0 → `D_800699D2` = 0xFF
  (actor 1), else `D_800699D0` (actor 0). Actor p reads player p. No voice-line function and
  no `rand()`; the scripts' own SE commands are the voices.
- Mood (`func_80031BB0` / `C5C`): mood < 9 → `func_80031D08` / `F44`; mood 9 →
  `func_80031E28` / `2064`; ≥ 10 kept. A different result restarts the actor.
  - Over `D_80069694` (1 = player 0 topped out, 2 = player 1 or both; `func_8002DD28`): the
    loser's actor → 11, the winner's → 10.
  - (mood < 9 only) `D_80100640[p]` (blobs the last garbage drop put in) ≥ 6 → 9. Mood 9 holds
    until the actor's done flag `D_8005F3D4/8` is set, then falls through to the rest.
  - Chain `D_800C3068 + 0x30·p` ≥ 2: if ≠ last seen `D_8005F630/4`, store it and mood =
    min(chain + 2, 8) with the current mood zeroed first (so it always restarts); else keep.
  - Chain < 2: last seen = 0; own n = `D_801005FA[p]` + `D_80100634[p]`: ≥ 45 → 3, ≥ 26 → 2,
    else the opponent's n ≥ 26 → 1, else 0.
  - Mood 9 plays once per drop: the garbage shake `func_8001A8B8` (see "Garbage"), run by the
    frame draw after `func_8003121C`, zeroes `D_80100640/2` on the frame after the drop's first
    blob lands, so when the done flag comes the mood falls to 0–3 and stays there. (Without the
    shake the 6+ drop would send it back to 9 about every 95 frames until the CPU's turn ends;
    an earlier harness stubbed the frame draw and showed exactly that.) Checked against the
    original by `python3 tools/framediff.py actor` with `stackers-web`
    `vs_cpu::actor_dump::frame_dump` (mood, done flag, drop and the actor's sound-op SEs; 4
    stages, ≈ 3,300 frames, CPU garbage forced at frames 60 / 700 / 1400).
- Art on screen: `D_800662F0` is bumped on the winning frame, but the beaten stage's background
  and sheets stay in VRAM (and its actor table, read once by `func_800319CC`) until the next
  stage's init and load, so the results show over the stage just won (`CpuMatch::art_stage`).
- Art and voices per stage (`func_80018ADC`, called by the stage load `func_80030394` state 4
  after `func_80017EB0`): SE kind 3 preload (bank) and files into RAM `0x80180000` / `0x80189000`
  / `0x80192000` / `0x8019B000`:
  0 AMIGO 0x89–0x8B (VAMIGO13/23/33), 1 GAMER 0x8F–0x91 (VGAKI1–3), 2 NEKO 0x95–0x97 (VNEKO1–3),
  3 AMIGO 0x83–0x85 (VAMIGO1–3), 4 DENKYU 0x8C–0x8E, 5 GAMER 0x92–0x94 (VGAKI11/21/31),
  6 AMIGO 0x86–0x88 (VAMIGO11/21/31), 7 A_TOS 0x80–0x82 (VA_TOS11/21/31), 8 NEKO 0x98–0x9A
  (VNEKO12/22/32), 9 NAGAI 0x9B–0x9E (VAKUTEN1–4), 10 A_TOS 0x7D–0x7F (VA_TOS1–3), 11 NEKO
  0x9F–0xA1 (VNEKO11/21/31), 12 TAMASAMA 0xA2–0xA5 (VTAMA1–4), 13 OHSAMA 0xA6–0xA9 (VKING1–4).
  Names from `D_80057298` (SD_TIM / SD_TIM2 folders); banks are `SOUND/VAB_CHR`. The SD
  scripts (Ready/Go, win panel) upload from the same RAM, so the port's fixed VAMIGO13 pages
  are wrong for every stage but 0.
- Actor upload command 7 (`func_80032BCC`): 0/1/2/3 → RAM slot 1/2/3/4 as page 26, 4 → slot 3,
  5 → slot 4, 6 → slot 1 as page 27 (`sdchr` currently ignores 3 and 5: slot 4 is used by
  stages 9, 12, 13). Scripts sit at (−24, 24) (stage 9: (−32/0, 16), two objects).
- Port notes: the field counts are `cpu::field_blobs` (as for the AI). Pages 26/27 are tracked
  per VM (SD stage and actor separately), while the original shares one VRAM spot between them
  (last upload wins). play.html loads the eight VAB_CHR banks as optional SOUND entries.

## Ending (verified, mode 16)

Implemented in `stackers_core::ending`, `hud::ending_page` / `ending_file_path`, web `Ending` +
`Screen::ending` (page ids 600 + file index); `CpuMatch::ending` hands over from mode 13.
play.html's level picker has "Ending: …" entries; screen example mode `ending` (arg = course,
+10 for a continued course).

- `func_80030798`, state `D_80100410` (`jtbl_80011ED0`), timer `D_8005F3B4`. Every drawn frame
  runs `func_8002EC34` (start a requested script) + `func_8002EC00` (SD step) with SD draw mode
  3 (`D_8005EC8C`: frame palette, brightness `D_80103610[slot]`), slot 0 on top, no background.
- 0: `func_80030CF0` loads KABEMONO (file 0x27) → page 20, HIRAPANE (0xE) → 21, and by course
  `D_800C30C8`: 0 → VAMIGOE2/VGAKIE2/VNEKOE2 (0x32/0x35/0x18) → 0/1/2; 1–2 → VAMIGOE 0,
  VDENKYUE 1, VGAKIE 2, VAKUTENE 3, VA_TOSE 5, VNEKOE 7, VTAMAE 9, VKINGE 10 (0x31, 0x33, 0x34,
  0x30, 0x2F, 0x36, 0x37, 0x38). XA 9 (`func_80040A7C(9, 1)`). Scene `D_800C3514` and count
  `D_8005EC90`: `D_800C2FE0` = 14 (cleared without a continue) → 0x153 / 16 ("Game Clear — You
  are master!", the king too); else course 0 → 0x150 / 5 ("Practice is over"), 1 → 0x152 / 14,
  2 → 0x154 / 14 ("Congratulations"); other courses → mode 0. Brightness `[0..count]` = 0x80,
  `[count]` = 0x40 (the KABEMONO wall, spinning by negative x scale).
- 1: 0x259 frames. 2: slots below count −2, the wall −1 per frame until slot 0 is 0 (64).
- 3: course 0 → mode 0 (no staff roll); else objects `0..count` killed (`+8` = −1). 4:
  `func_80030F7C` loads STAFF1–9 (0x39–0x41) → pages 0–8, ENDKABE1–5 (0x42–0x46) → 21–25. 5:
  XA 8, staff page `D_8005F3B8` = 0x155, brightness 0/1 = 0x80.
- 6: page 0x155–0x15E, each 0x21D drawn frames then one blank frame; 0x15E sets slot 1 to 0x40
  (its wall is KABEMONO). At 0x15F → mode 0 (MSG2 reloaded into page 22, title).
- States 0, 3, 4, 5 draw nothing. The ending takes no input.
- SD anim command 4 (`func_8002F814`) sign-extends its targets (the port read them unsigned,
  which blew up the wall's −128 scale).
- XA music: entries 9 and 8 (`Ending::xa`, played by `web/play.html`).
- Frame diffed (`python3 tools/framediff.py ending`, Rust `stackers-web` `ending::tests::frame_dump`):
  state, timer, staff page, brightness, XA, SEs and all 25 SD objects (`D_8005ECA4`, 0x48 bytes:
  +0 x, +4 y, +8 layer, +0xC flip, +0x1C frame record, +0x24/+0x26 scale) match for courses 0–3
  with last stage 3/13/14. State 0 only requests the scene (`D_800C3514`); `func_8002EC34`
  starts it on state 1's first frame.

## Option (verified, mode 17)

Implemented in `stackers_core::front::option` (drawn by `Screen::title`; `examples/title.rs`
reaches it with `start@400,start@500,up@580,cross@600`, then `cross@720` for Sound or
`down@710,cross@720` for Controller).

- Entered from the menu (state 1 at t = 0x3C): script 0x15F, palettes slot 0 = 3, 1–2 = 0, 3–5 = 2.
  `func_8003DDD8`: sub-state `D_800C34AA` 0 inits (cursor 0, `D_80100368` = {0x60, 0xA0}, arrow
  (0x50, 0x60) frame 5, `D_800C2FB8` = 0, `func_8003F140`). Page `D_800C2EC0`, phase `D_800C2ECA`
  (0 in, 1 live, 2 out), timer `D_801003DC` (+1 first). Every frame ends with the SD VM
  (`func_8002EAD4`), the background and the menu arrow (`func_8001B860`, clut 3, layer 0) at
  (`D_800B1344`, `D_800B1348`); pages 0 and 1 set the arrow y to `D_80100368[cursor]` last, so
  the "y −2" of the fly-off never shows.
- Page 0, the list (Sound / Controller): phase 0 resets at t = 1, live from t ≥ 0x15. Phase 1: △ on
  either pad → phase 2 (script 0x160, pick 0); otherwise pad 2 then pad 1, pressed word == ↑/↓
  moves (wrap over 2, SE 0xA7), == Start or ✕ confirms: SE 0xA8, slot 1 or 2 palette 1, pick
  `D_800B1282` = 1 or 2, arrow frames 5 → 0x1F, then script 0x160 and phase 2. Phase 2: arrow x −16
  per frame for t < 0x15, then pick 0 → mode 8 (script 0x105, the menu palettes) or page = pick.
- Page 1, Sound: t = 1 → ys {0x3C, 0x96}, palettes 0 = 3, 3–4 = 2, others 0, script 0x161, tiles
  init (`func_8003EE4C`). Item 0 = "SE Volume" `D_80100388` (= `D_80100358`), item 1 = "BGM
  Volume" `D_80103690` (= `D_80103244`); both 0x40 at boot (`func_80040394`), 0–0x7F. Phase 1: △
  → phase 2 (script 0x162, the rest still runs); ↑/↓ on the OR of both pressed words; ←/→ from
  `func_8003EBF4` (per pad: on the press, then after 31 frames every other frame while that
  direction alone is held; counters `D_800C2F5C`) step both channels by one with SE 0xA7.
  Item 1 also calls `func_8004D130(0, l, r)` every frame = SPU CD input volume (attr 0xC0), so
  it scales CD-DA and XA alike. The SE volume is the left/right volume of every key-on
  (`func_800407C4` → `func_8004D578`). Phase 2: arrow flies for t < 0x15, then script 0x15F,
  palettes 0 = 3, 1–2 = 0, page 0.
- The bars (`func_8003ED18`, layer-1 TILEs `D_800C3408`, 0x10 bytes, 4 per buffer; positions are
  relative to the (160, 120) draw offset): x −0x20, y −0x14 and 0x46, h 0x10. Tiles 0/1 are
  blue (0, 0, 0xFF), w = the volume; tiles 2/3 are (0x80, 0x80, 0xFF), w 0x80, half-transparent,
  added last so drawn first. They fit the script's Min … Max frame exactly.
- Page 2, Controller: `D_80100374[pad]` (zeroed at boot by `func_80014C30`) = layout 0–2
  ("Type A/B/C"). t = 1: arrow x 0x140 (off-screen), script 0x163, palettes 0 = 3, 1–6 = 2,
  7–9 = 0, cursors from the layouts, `func_8003F140` + `func_8003F400`. Phase 0 moves the
  layout numbers +4 per frame for t = 1 … 0x14 (live at 0x14); phase 2 moves them −4 for t <
  0x15, then script 0x15F, palettes 0 = 3, 1–2 = 0, 7–8 = 2, page 0. Phase 1: △ → phase 2
  (script 0x164); per pad pressed == → / ← steps its layout (wrap over 3, SE 0xA7).
- Controller drawing (main OT `D_800C34BC`, in front of the scripts; OPTION.TIM, tpage (384, 0)
  4-bit from the DR_MODE `D_80066B88`): the layout number per pad, SPRT `D_8010AEA8` 16×16,
  u = 0x40 + 16·layout, v 0x30, CLUT `D_80103410` = OPTION's row 0 (`D_80103250[14][0]`), x
  −0x4C / 0x54, y from −0x86. Eight labels per pad (SPRT `D_800C3178`, CLUT (0, 501) = row 2):
  (u, v, w, h) from `D_800577AC`, x −0x8C / 0x14 (label 5 +0x28), y = `D_8005777C[layout][i]` −
  0x78.
- The remap (`func_80015548(word, pad)`, called by `func_80014F20` per mode via
  `jtbl_80010080`): word & 0xF0FF, each bit from 0x8000 down becomes `D_80057028[layout][i]`.
  Layout 1 swaps □ ↔ ✕, layout 2 also ○ ↔ △ (the labels agree: Type A ✕ counter-clockwise,
  ○ clockwise, △ change view, □ bomb). Modes 4, 7, 11 remap pad 1's word with the menu pad's
  layout, 13 the menu pad's (the CPU's word is copied raw), 14/15 each pad with its own; the
  results go to `D_80100240` (held), `D_800C34F8` (pressed) and `D_800C34F0`. Pause and the menus
  read the raw `D_800B1908`. The port keeps Start/Select in the remapped word (`remap`), applies
  `Game::layout` / `Versus::layouts` to the play and the camera only, and play.html keeps the
  settings in localStorage (the original has no save).

## Whole-game differential check (modes 4, 14, 13, 11, 7 and 2)

`tools/framediff.py` replays games through the original pad reader `func_80014F20` and mode
handler `func_80017098` in `tools/mips.py` (BLT_ANM.OBJ loaded at `0x801C0000`, drawing and HUD
stubbed, BIOS rand hooked) and compares, after every frame, the player struct, the queue, the
rand seed and every cell with `stackers_core` (`game::tests::frame_dump`, random held pads
including L1, until game over). Cells' animation done flag (`+0x28`) is compared only above
state 2: the original leaves it set after a retract and nothing reads it there. 10 games
(≈ 1,000–3,000 frames each) match exactly after the `func_80034994` fix (see "Matching").

Two players: `python3 tools/framediff.py match <dump>` with
`set_match::tests::frame_dump` runs the set match (mode 14 handler `func_80015FCC`, sub-state 0
init then play) with random held pads on both sides (□ rarely, random difficulties and
bombs, one set, until it ends) and also compares each player's garbage counters, special
column, bomb stock and score. Not stubbed (they carry rules state): the strip
`func_8003BCD0` (special column), the cut-ins `func_80038A08` (a bomb waits for their end;
SD_CHR.OBJ loaded at `0x801A8000`) and `func_8002C770` (last frame's chain values, which
start the indicator spin); the main loop's `func_8003D830` is called before the handler (`D_80066AD0`
drives the garbage divisor). The sprite drawers (`func_8001B148` … `func_8001D8C4`, every caller
of the libgpu prim setters there) are. Found: special blobs pop on landing (see "Special
blobs"), and a blocked garbage column returns its rest to the pool (see "Garbage"). After those, 16 sets
(12 with `FRAME_SEED=4242`, 1,165–4,530 frames each, ≈ 37,000 frames) match exactly.

vs CPU: `python3 tools/framediff.py cpu <dump>` with `vs_cpu::tests::frame_dump` runs mode 13
(handler `func_80016480`) on a random course, stage and menu pad, with random held pads on the
human's side and the AI (`func_80034C60`, inside the pad reader) on the other, through play and
the stage-won (sub-state 2) or lost (sub-state 4) sequence. The harness sets `D_800B0050` =
1 − menu pad (the pad the AI's word replaces; the Arcade menu sets it, not `D_800C2FB8`) and
`D_80100688` = stage (art loaded, so sub-state 0 goes straight to play). Also stubbed: the actor
VRAM upload `func_80018770` + DrawSync `func_8004622C` (the mascot runs) and the stage art load
`func_80017EB0`. 20 games (2,422–10,569 frames, wins and losses) match exactly.
`FRAME_NEXT=<seed>` goes on into the next stage (half the games play the human's pad with the
AI, and Start comes now and then in the won / lost sequences: the stage load or a continue, then
300 frames of the next stage). Found: both write sub-state 0, so the init runs a frame later;
the init keeps the delays and soft-drop points (`Game::carry_from`, see "Set match"); the
continue's score + 1 is on the live score. With `FRAME_NEXT=9`, 12 games (≈ 63,000 frames, 11
into a second stage) match exactly.

Attract demo: `python3 tools/framediff.py demo <dump>` with `demo::tests::frame_dump` runs
mode 2 (handler `func_80018F24`, `D_801003DC` = 4 as the title leaves it) with no pads until it
returns to the title (ClearImage `func_800463C0` stubbed there). Found: the pad read updates
last frame's pads every frame, also in the panel and match-end sub-states where the fields
don't step, so a direction the AI held through a panel repeats (delay 2) instead of starting a
fresh move (6) (`Versus::hold`; the same applies to the pause in modes 11, 13 and 14). After
that, 8 demos (3,946 frames each) match exactly.

Time Trial: `python3 tools/framediff.py trial <dump>` with `trial::tests::frame_dump` runs mode 11
(handler `func_80016BB4`, FIL2_DAT loaded at `0x80170000`, frame and clock HUD stubbed); half
the games play pad 1 with the CPU AI (its own BIOS rand) so they reach a record. Found: player
1 in state 0 goes back to taking garbage (`func_8001E270`). 10 games match exactly.
`trialend` with `trial::tests::end_frame_dump` adds Start pauses, the pause menu and the end
sequence's presses, and compares the sub-state, pause menu, game-over state and the frame's SEs
(as a set, leaving out the mascot's actor-script sound op, return address `0x80032924`, as in
`cpupause`); the SD VM runs. 10 games (`FRAME_SEED` 1111, ≈ 61,000 frames, all to the countdown
and the course select, sub-state 12) match exactly; one reaches the original's freeze at
`func_8002F278` (see "Game over") and matches up to it. With `FRAME_PORTS=777`, 10 more (≈ 44,300
frames, 113 changes, 4 quitting from the menu) match exactly. Found: on the frame the pause menu
resumes, the mascot steps (see "Mascot" above).

Chain Reaction: `python3 tools/framediff.py chain <dump>` with `game::tests::chain_frame_dump`
runs mode 7 (handler `func_80015AAC`, the 79-row field compared as 78 rows; art, background,
tower and HUD stubbed) with □ grey drops. 4 games (≈ 116,000 frames) match with no fixes
needed. At ≈ 1 h per game in Python, split the dump per game and run them in parallel.

Solo through the end: `python3 tools/framediff.py end <dump>` with `game::tests::end_frame_dump`
plays solo games with Start pauses (menu ↑ ↓ ✕ △ Start, sometimes Start on Retry's leaving
frame) and runs them through game over and the countdown (✕ skips, sometimes right on a tick;
Start retries) to the restart or the title. It also compares the sub-state, the pause menu, the
sequence's phase / counter / limit / message and each frame's sound effects (one of each id per
frame, as `Game::sound` keeps them). Not stubbed: the chain sounds `func_8002C770` and the SD
VM `func_8002EB38` (it strips the request's high byte); SD_CHR.OBJ is loaded and the sprite
drawers are stubbed as for the set match. Entering `func_8002F278` is reported as the
original freezing. Found: a blob landing in its own slot is silent (see "Sound effects"); the
countdown's double request and the freeze it leads to (see "Game over"); Start on Retry's
leaving frame resumes play (see "Pause"). After those, 12 games (`FRAME_SEED` 555, ≈ 78,600
frames, 17 double steps, 6 resumes from Retry) and 8 more (9090, ≈ 45,900 frames, 14 double
steps) match exactly; three of them ran past "Game Over" and two of those froze at script 0x28,
matching up to the freeze. `FRAME_PORTS=<seed>` adds port connection changes to the end dumps
(`h/c` pad fields, as in the two-player pause dumps): with `FRAME_PORTS=777`, 12 solo games
(≈ 88,400 frames, 218 changes) match exactly, two up to the freeze, and 6 Chain Reaction games
(`chainend`, ≈ 68,900 frames, 174 changes) match exactly, one up to a freeze at request 0x2C.

Two-player pauses: `python3 tools/framediff.py matchpause <dump>` with
`set_match::tests::pause_frame_dump` (mode 14) and `cpupause` with
`vs_cpu::tests::pause_frame_dump` (mode 13) add Start pauses and menu presses (both pads in mode
14) and compare the sub-state, the menu state in `func_8002E79C`'s numbering, cursor, timer, the
pausing pad and the frame's sound effects (as a sorted set: the players' and the strip's calls
interleave differently). vs CPU leaves out SEs from the actor script's sound op
(`func_800328FC`, return address `0x80032924`): the mascot's voice lines come from the SD VM,
which the port runs in the web layer. Found: the versus menu is its own state machine (see "Set
match" → Pause), Start on either pad resumes in mode 14, and the strip steps on frames the menu
itself resumed. After those, 16 set matches (`FRAME_SEED` 31337, ≈ 52,500 frames, ≈ 2,900
paused, 3 quits) and 16 vs CPU stages (8086, ≈ 54,300 frames, ≈ 1,000 paused) match exactly.
`FRAME_PORTS=<seed>` adds port connection changes (a third pad field; the harness flips that
port's status byte `D_800B1440` / `D_800B1462` and saves both to `D_8006626C/D` after the
handler, as the main loop does). With `FRAME_PORTS=777` on the same seeds, 16 set matches (113
changes, ≈ 98 pausing from play, the changed port owning the menu) and 16 vs CPU stages (128
changes, ≈ 116 pausing) match exactly, as do the plain runs after the change.
`FRAME_CHALLENGER=<seed>` adds Start presses on the AI's pad in the vs CPU dump, half of them on
frames where the round ends anyway; the game's line stops at that frame. Found: the end check
runs after the challenger (see "vs CPU"). With `FRAME_CHALLENGER=5`, 12 stages (≈ 24,400 frames:
8 to sub-state 10, 3 overridden to 4, 1 pressed while paused) match exactly.

Challenger match: `python3 tools/framediff.py challenger <dump>` with
`set_match::tests::challenger_frame_dump` runs mode 15 from vs CPU's hand-over (menu pad, course
stage and both scores random) through every set, the set results, pauses, the countdown (the
loser's Start rematches) and the return to mode 13. The SD VM runs (it consumes the countdown's
requests; its sound op, return address `0x8002F940`, is left out like the actor's). Found: the
divisor's head start is mode 14's alone (see "Set match"), the set result / rematch / re-pick
write sub-state 0 so the init is a frame later, the init keeps the player delays and soft-drop
points, and mode 15's rematch doesn't add the score + 1. 8 matches (`FRAME_SEED` 1515,
≈ 174,700 frames, all back to mode 13) match exactly.

## Open questions

None at the moment.
