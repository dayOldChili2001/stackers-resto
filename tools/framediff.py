"""Differential check of a whole solo game (mode 4) against the original, frame by frame.

    FRAME_DUMP=/tmp/f.txt cargo test -p stackers-core game::tests::frame_dump -- --ignored
    python3 tools/framediff.py /tmp/f.txt

Two players, set match (mode 14, handler func_80015FCC):

    FRAME_DUMP=/tmp/m.txt cargo test -p stackers-core set_match::tests::frame_dump -- --ignored
    python3 tools/framediff.py match /tmp/m.txt

vs CPU (mode 13, handler func_80016480; the AI runs inside the pad reader):

    FRAME_DUMP=/tmp/c.txt cargo test -p stackers-core vs_cpu::tests::frame_dump -- --ignored
    python3 tools/framediff.py cpu /tmp/c.txt

Attract demo (mode 2, handler func_80018F24; the AI plays both sides, no pads):

    FRAME_DUMP=/tmp/d.txt cargo test -p stackers-core demo::tests::frame_dump -- --ignored
    python3 tools/framediff.py demo /tmp/d.txt

Time Trial (mode 11, handler func_80016BB4; player 1 is the target field, no pad):

    FRAME_DUMP=/tmp/t.txt cargo test -p stackers-core trial::tests::frame_dump -- --ignored
    python3 tools/framediff.py trial /tmp/t.txt

Chain Reaction (mode 7, handler func_80015AAC; a 79-row field, □ drops grey pairs):

    FRAME_DUMP=/tmp/r.txt cargo test -p stackers-core game::tests::chain_frame_dump -- --ignored
    python3 tools/framediff.py chain /tmp/r.txt

Solo with pauses, the pause menu and the game-over sequence (until the game is left):

    FRAME_DUMP=/tmp/e.txt cargo test -p stackers-core game::tests::end_frame_dump -- --ignored
    python3 tools/framediff.py end /tmp/e.txt

Chain Reaction with pauses, the pause menu and its end sequence:

    FRAME_DUMP=/tmp/x.txt cargo test -p stackers-core game::tests::chain_end_frame_dump -- --ignored
    python3 tools/framediff.py chainend /tmp/x.txt

Pauses and the two-player pause menu (`func_8002E79C`), set match / vs CPU:

    FRAME_DUMP=/tmp/p.txt cargo test -p stackers-core set_match::tests::pause_frame_dump -- --ignored
    python3 tools/framediff.py matchpause /tmp/p.txt
    FRAME_DUMP=/tmp/q.txt cargo test -p stackers-core vs_cpu::tests::pause_frame_dump -- --ignored
    python3 tools/framediff.py cpupause /tmp/q.txt

Time Trial with pauses, the pause menu and its end sequence:

    FRAME_DUMP=/tmp/u.txt cargo test -p stackers-core trial::tests::end_frame_dump -- --ignored
    python3 tools/framediff.py trialend /tmp/u.txt

The challenger match (mode 15) through all three sets, the countdown and the way out:

    FRAME_DUMP=/tmp/r.txt cargo test -p stackers-core set_match::tests::challenger_frame_dump -- --ignored
    python3 tools/framediff.py challenger /tmp/r.txt

The ending (mode 16, handler func_80030798) for each course / last stage pair, SD objects too:

    FRAME_DUMP=/tmp/n.txt cargo test -p stackers-web --lib ending::tests::frame_dump -- --ignored
    python3 tools/framediff.py ending /tmp/n.txt

With FRAME_PORTS=<seed> both pause dumps also flip the ports' connection state now and then
(a third pad field), which pauses like Start while the clock runs.

Each game is replayed from the same BIOS rand seed and pad sequence through the original pad
reader `func_80014F20` and mode handler `func_80017098` (drawing stubbed out), and the player,
queue, rand seed and cells are compared after every frame. Reports the first divergence per game.
"""
import os
import sys
sys.path.insert(0, os.path.dirname(__file__))
from mips import *

DATA = os.path.join(os.path.dirname(__file__), "..", "extracted", "disc", "DERODERO", "DATA")
ANM = os.path.join(DATA, "BLT_ANM.OBJ")
SD_CHR = os.path.join(DATA, "SD_CHR.OBJ")
# Drawing, HUD and screen setup: no effect on the rules.
STUBS = [0x80018464, 0x80018E68, 0x8002CB98, 0x80038A08, 0x8001A7BC, 0x8002CBC0, 0x800195FC,
         0x800258A8, 0x8002C770, 0x8002CBE8, 0x8002CC88, 0x8003DCB8, 0x8002CC90, 0x8001DEF0,
         0x800194D4, 0x80023294]
SE = 0x80040740
P = 0x80100250


def new_cpu(seed, sounds):
    cpu = Cpu()
    anm = open(ANM, "rb").read()
    cpu.mem[0x1C0000:0x1C0000 + len(anm)] = anm
    st = {"seed": seed}

    def rand_hook(c):
        st["seed"] = (st["seed"] * 0x41C64E6D + 0x3039) & 0xFFFFFFFF
        c.r[2] = (st["seed"] >> 16) & 0x7FFF
    cpu.hooks[0x800524A4] = rand_hook

    def nop(c):
        c.r[2] = 0
    for a in STUBS:
        cpu.hooks[a] = nop

    def se(c):
        sounds.append(s32(c.r[4]))
        c.r[2] = 0
    cpu.hooks[SE] = se
    cpu.seth(0x80100684, 4)
    cpu.seth(0x800C34AA, 0)
    return cpu, st


def frame(cpu, held):
    v = ~held & 0xFFFF
    cpu.wr(0x800B1443, 1, v & 0xFF)
    cpu.wr(0x800B1442, 1, v >> 8)
    cpu.wr(0x800B1464, 1, 0xFF)
    cpu.wr(0x800B1465, 1, 0xFF)
    cpu.call(0x80014F20)
    cpu.call(0x8003D830)  # the main loop's clocks: D_80066AD0 (play frames) in sub-state 1
    cpu.call(0x80017098)


def line(cpu, st, p=0, rows=18):
    h = cpu.h
    P = 0x80100250 + 0x50 * p
    s = "%d %d %d %d %d %d %d %d %d %d %d %d %d %d |" % (
        h(P + 0xC), s32(cpu.w(P)), s32(cpu.w(P + 4)), cpu.w(P + 8), h(P + 0x12), h(P + 0xE),
        h(P + 0x10), h(P + 0x14), h(P + 0x16), h(P + 0x18), h(P + 0x1A), h(P + 0x1C),
        h(P + 0x1E), h(P + 0x20))
    for k in range(3):
        q = 0x800699F0 + 0x90 * p + 0x30 * k
        s += " %d,%d" % (h(q + 0x18), h(q))
    s += " | %d |" % st["seed"]
    for c in range(1, 7):
        for r_ in range(rows):
            a = cell(p, c, r_)
            state, color = h(a), h(a + 2)
            if state != 0 or color != 0x80:
                s += " %d,%d:%d:%d:%d:%d:%d:%d:%d:%d:%d:%d" % (
                    c, r_, state, color, h(a + 4), h(a + 8), s32(cpu.w(a + 0xC)), s32(cpu.w(a + 0x10)),
                    h(a + 0x2C), int(h(a + 0x2A) == -1), h(a + 0x16), int(h(a + 0x28) != 0 and state > 2))
    return s


# Mode 14 (set match): handler func_80015FCC. Art loads, drawing and the SD VM are stubbed; the
# strip (func_8003BCD0, special column), cut-ins and mascot init run.
MATCH_STUBS = [0x80017B20, 0x80018ADC, 0x8001A82C, 0x8001DB4C, 0x800194D4, 0x800195FC, 0x800258A8,
               0x8002EB38, 0x80023294, 0x8001DA4C, 0x80018A08,
               # The sprite drawers (every caller of the libgpu prim setters in this range).
               0x8001B148, 0x8001B4D0, 0x8001B860, 0x8001BC78, 0x8001C03C, 0x8001C414,
               0x8001C7A8, 0x8001CBE4, 0x8001CD40, 0x8001D8C4,
               # The actor VRAM upload (mascots, mode 13).
               0x80018770, 0x8004622C, 0x80017EB0,
               # ClearImage on the demo's way back to the title.
               0x800463C0]


def new_match(seed, diff_, bombs, sounds):
    cpu, st = new_cpu(seed, sounds)
    for a in MATCH_STUBS:
        cpu.hooks[a] = cpu.hooks[STUBS[0]]
    # The cut-ins end a bomb (state 12 waits for their flag); drawing is separate. Their
    # scripts are in SD_CHR.OBJ.
    del cpu.hooks[0x80038A08]
    # func_8002C770 keeps last frame's chain values (D_80066798, D_80101EC8): the indicator
    # spin starts from them.
    del cpu.hooks[0x8002C770]
    sd = open(SD_CHR, "rb").read()
    cpu.mem[0x1A8000:0x1A8000 + len(sd)] = sd
    cpu.seth(0x80100684, 14)
    for p in range(2):
        cpu.seth(0x80102740 + 2 * p, diff_[p])
        cpu.seth(0x800FFCF8 + 2 * p, bombs[p])
    cpu.seth(0x80103692, 1)  # one set
    cpu.setw(0x800B137C, 0)
    cpu.setw(0x800C2FB8, 0)
    return cpu, st


def new_vs_cpu(seed, me, course, stage, last, sounds):
    cpu, st = new_match(seed, (0, 0), (0, 0), sounds)
    cpu.seth(0x80100684, 13)
    cpu.setw(0x800C2FB8, me)
    cpu.setw(0x800B0050, 1 - me)  # the pad the AI replaces (set by the Arcade menu)
    cpu.seth(0x800C30C8, course)
    cpu.seth(0x800662F0, stage)
    cpu.seth(0x800C2FE0, last)
    cpu.seth(0x80100688, stage)  # the stage art is loaded: sub-state 0 goes straight to play
    # The next stage's art (sub-state 11, func_80030394 → func_80017EB0) for FRAME_NEXT dumps.
    for a in (0x80017EB0, 0x80018E60):
        cpu.hooks[a] = cpu.hooks[STUBS[0]]
    cpu.setw(0x80069658, 0)
    cpu.setw(0x8006965C, 0)
    return cpu, st


def new_challenger(seed, me, stage, scores, sounds):
    """Mode 15 as vs CPU's sub-state 10 leaves it (`0x80016968`)."""
    cpu, st = new_match(seed, (3, 3), (0, 0), sounds)
    # The SD VM consumes the countdown's requests D_800C3514 (as in the solo end diff).
    del cpu.hooks[0x8002EB38]
    cpu.watch[0x8002F278] = frozen
    # Its sound op (return address 0x8002F940) plays in the port's web layer (SdStage).
    se = cpu.hooks[SE]
    cpu.hooks[SE] = lambda c, se=se: c.r.__setitem__(2, 0) if c.r[31] == 0x8002F940 else se(c)
    cpu.seth(0x80100684, 15)
    cpu.seth(0x80103692, 3)
    cpu.setw(0x800B137C, 1)
    cpu.setw(0x800C2FB8, me)
    cpu.setw(0x800B0050, 1 - me)
    cpu.seth(0x800C33F8, stage)
    cpu.seth(0x800662F0, 0)
    cpu.seth(0x80100688, 0)
    cpu.seth(0x800C30C8, 1)
    cpu.seth(0x800C2FE0, 14)
    for p in range(2):
        cpu.setw(0x80069658 + 4 * p, scores[p])
        cpu.seth(0x8010ADFC + 2 * p, 0)
        cpu.seth(0x800699C4 + 2 * p, 0)
    return cpu, st


def new_demo(seed, sounds):
    cpu, st = new_match(seed, (0, 0), (0, 0), sounds)
    cpu.seth(0x80100684, 2)
    cpu.seth(0x80103692, 0)
    cpu.setw(0x801003DC, 4)  # left by the title's state 5
    cpu.seth(0x80100360, 0)
    return cpu, st


FIL2_DAT = os.path.join(DATA, "FIL2_DAT.OBJ")


def new_trial(seed, course, sounds):
    cpu, st = new_match(seed, (0, 0), (0, 0), sounds)
    fil = open(FIL2_DAT, "rb").read()
    cpu.mem[0x170000:0x170000 + len(fil)] = fil
    cpu.hooks[0x8001A70C] = cpu.hooks[STUBS[0]]  # the frame / monitor draw
    cpu.hooks[0x8003D9B0] = cpu.hooks[STUBS[0]]  # the clock HUD
    cpu.seth(0x80100684, 11)
    cpu.seth(0x80103692, 0)
    cpu.wr(0x800699D4, 1, course)
    for k in range(5):
        cpu.setw(0x800C30D0 + 4 * k, 0)
    cpu.seth(0x80100688, 0)  # the course art is loaded (D_800662F0 = 0 after the init)
    return cpu, st


def main_trial():
    lines = open(sys.argv[2]).read().split("\n")
    i = games = bad = 0
    while i < len(lines):
        if not lines[i].startswith("T "):
            i += 1
            continue
        _, seed, course, pads = lines[i].split()
        seed, course = int(seed), int(course)
        pads = [int(x) for x in pads.rstrip(",").split(",")]
        want = [l[2:] for l in lines[i + 1:i + 2 + len(pads)]]
        i += 1 + len(want)
        sounds = []
        cpu, st = new_trial(seed, course, sounds)
        match_frame(cpu, (0, 0), 0x80016BB4)  # sub-state 0: the init
        games += 1
        for n, held in enumerate([None] + pads):
            if held is not None:
                match_frame(cpu, (held, 0), 0x80016BB4)
            got = match_line(cpu, st, special=False)  # no special column in mode 11
            if got != want[n]:
                bad += 1
                print(f"game {games} seed {seed} course {course} frame {n} pad {held} sub-state {cpu.h(0x800C34AA)}:")
                for d in match_diff(got, want[n]):
                    print("  ", d)
                break
        else:
            print(f"game {games}: {len(pads)} frames match, sub-state {cpu.h(0x800C34AA)}")
        sys.stdout.flush()
    print(games, "games", bad, "diverged")


def main_trial_end():
    lines = open(sys.argv[2]).read().split("\n")
    i = games = bad = 0
    while i < len(lines):
        if not lines[i].startswith("U "):
            i += 1
            continue
        _, seed, course, pads = lines[i].split()
        seed, course = int(seed), int(course)
        pads = [tuple(int(y) for y in x.split("/")) for x in pads.rstrip(",").split(",")]
        want = [l[2:] for l in lines[i + 1:i + 2 + len(pads)]]
        i += 1 + len(want)
        sounds = []
        cpu, st = new_trial(seed, course, sounds)
        # The countdown's requests go through the SD VM, as in the solo end diff.
        del cpu.hooks[0x8002EB38]
        cpu.watch[0x8002F278] = frozen
        # The mascot's lines come from its actor script's sound op (as in cpupause).
        se = cpu.hooks[SE]
        cpu.hooks[SE] = lambda c, se=se: c.r.__setitem__(2, 0) if c.r[31] == 0x80032924 else se(c)
        match_frame(cpu, (0, 0), 0x80016BB4)  # sub-state 0: the init
        games += 1
        try:
            for n, held in enumerate([None] + pads):
                sounds.clear()
                if held is not None:
                    match_frame(cpu, (held[0], 0, held[1] if len(held) > 1 else 0), 0x80016BB4)
                body, tail = want[n].split("#U ")
                got_body = match_line(cpu, st, special=False)
                h, w = cpu.h, cpu.w
                got_tail = "%d %d %d %d %d %d %d %d [%s]" % (
                    h(0x800C34AA), w(0x80103248), (cpu.rd(0x800B1400, 1) ^ 0x80) - 0x80, w(0x8006850C),
                    h(0x800B1420), w(0x800C3530), w(0x800B1A28), h(0x800C3514) & 0xFF,
                    ", ".join(str(s) for s in sorted({s & 0xFF for s in sounds})))
                # Leaving: the next screen's state, so the fields alone.
                ok = got_body == body and (tail.startswith("-1 ") or got_tail == tail)
                if not ok:
                    bad += 1
                    print(f"game {games} seed {seed} course {course} frame {n} pad {held}:")
                    for d in match_diff(got_body, body):
                        print("  ", d)
                    print("   emu  #", got_tail)
                    print("   rust #", tail)
                    break
            else:
                print(f"game {games}: {len(pads)} frames match, mode {cpu.h(0x80100684)} sub-state {cpu.h(0x800C34AA)}")
        except Frozen:
            print(f"game {games}: froze at frame {n} (func_8002F278)")
        sys.stdout.flush()
    print(games, "games", bad, "diverged")


def new_chain(seed, sounds):
    cpu, st = new_match(seed, (0, 0), (0, 0), sounds)
    # Art, background, tower / HUD drawing, the background layer tie-in.
    for a in (0x800179A8, 0x8002430C, 0x8001A270, 0x8002466C, 0x8002C98C, 0x8001A2B4):
        cpu.hooks[a] = cpu.hooks[STUBS[0]]
    cpu.seth(0x80100684, 7)
    cpu.seth(0x80103692, 0)
    cpu.seth(0x800C34AC, 0)  # no preset
    cpu.setw(0x80067224, 0)  # record
    cpu.seth(0x80100688, 0)
    return cpu, st


def main_chain():
    lines = open(sys.argv[2]).read().split("\n")
    i = games = bad = 0
    while i < len(lines):
        if not lines[i].startswith("R "):
            i += 1
            continue
        _, seed, pads = lines[i].split()
        seed = int(seed)
        pads = [int(x) for x in pads.rstrip(",").split(",")]
        want = [l[2:] for l in lines[i + 1:i + 2 + len(pads)]]
        i += 1 + len(want)
        sounds = []
        cpu, st = new_chain(seed, sounds)
        match_frame(cpu, (0, 0), 0x80015AAC)  # sub-state 0: the init
        games += 1
        for n, held in enumerate([None] + pads):
            if held is not None:
                match_frame(cpu, (held, 0), 0x80015AAC)
            got = line(cpu, st, rows=78)
            if got != want[n]:
                bad += 1
                print(f"game {games} seed {seed} frame {n} pad {held} sub-state {cpu.h(0x800C34AA)}:")
                for d in diff(got, want[n]):
                    print("  ", d)
                break
        else:
            print(f"game {games}: {len(pads)} frames match, sub-state {cpu.h(0x800C34AA)}")
        sys.stdout.flush()
    print(games, "games", bad, "diverged")


def end_line(cpu, st, sounds, rows=18):
    """`line` plus the sub-state, pause menu, game-over sequence and the frame's SEs."""
    h, w = cpu.h, cpu.w
    return "%s # %d %d %d %d %d %d %d %d [%s]" % (
        line(cpu, st, rows=rows), h(0x800C34AA), w(0x80103248), ((cpu.rd(0x800B1400, 1) ^ 0x80) - 0x80), w(0x8006850C),
        h(0x800B1420), w(0x800C3530), w(0x800B1A28), h(0x800C3514) & 0xFF,
        # Game::sound keeps one of each id per frame (the same sample started twice at once).
        ", ".join(str(s) for s in dict.fromkeys(s & 0xFF for s in sounds)))


class Frozen(Exception):
    """The original entered func_8002F278 (`j` to itself, SD motion op 0): the console hangs."""


def frozen(cpu):
    raise Frozen()


def new_end(seed, sounds):
    cpu, st = new_cpu(seed, sounds)
    for a in MATCH_STUBS:
        cpu.hooks[a] = cpu.hooks[STUBS[0]]
    # The chain sounds (func_8002C770) and the game-over SD scripts (SD_CHR.OBJ) run.
    # The SD VM func_8002EB38 consumes the requests D_800C3514 that the countdown adds to.
    for a in (0x8002C770, 0x8002EB38):
        del cpu.hooks[a]
    sd = open(SD_CHR, "rb").read()
    cpu.mem[0x1A8000:0x1A8000 + len(sd)] = sd
    cpu.watch[0x8002F278] = frozen
    return cpu, st


def new_chain_end(seed, sounds):
    cpu, st = new_chain(seed, sounds)
    del cpu.hooks[0x8002EB38]  # the SD VM consumes the end sequence's requests
    cpu.watch[0x8002F278] = frozen
    return cpu, st


def main_end(chain=False):
    lines = open(sys.argv[2]).read().split("\n")
    tag, new, handler, rows = ("X ", new_chain_end, 0x80015AAC, 78) if chain else ("E ", new_end, 0x80017098, 18)
    i = games = bad = 0
    while i < len(lines):
        if not lines[i].startswith(tag):
            i += 1
            continue
        _, seed, pads = lines[i].split()
        seed = int(seed)
        pads = [tuple(int(y) for y in x.split("/")) for x in pads.rstrip(",").split(",")]
        want = [l[2:] for l in lines[i + 1:i + 2 + len(pads)]]
        i += 1 + len(want)
        sounds = []
        cpu, st = new(seed, sounds)
        match_frame(cpu, (0, 0), handler)  # sub-state 0: the init
        sounds.clear()
        games += 1
        for n, held in enumerate([None] + pads):
            if held is not None:
                sounds.clear()
                try:
                    match_frame(cpu, (held[0], 0, held[1] if len(held) > 1 else 0), handler)
                except Frozen:
                    print(f"game {games}: {n - 1} frames match, then the original freezes (SD motion op 0, "
                          f"request {cpu.h(0x800C3514):#x})")
                    break
            got = end_line(cpu, st, sounds, rows)
            if got != want[n]:
                bad += 1
                print(f"game {games} seed {seed} frame {n} pad {held}:")
                for d in diff(got.split(" # ")[0], want[n].split(" # ")[0]):
                    print("  ", d)
                print("   emu  #", got.split(" # ")[1])
                print("   rust #", want[n].split(" # ")[1])
                break
        else:
            print(f"game {games}: {len(pads)} frames match, mode {cpu.h(0x80100684)} sub-state {cpu.h(0x800C34AA)}")
        sys.stdout.flush()
    print(games, "games", bad, "diverged")


def main_demo():
    lines = open(sys.argv[2]).read().split("\n")
    i = games = bad = 0
    while i < len(lines):
        if not lines[i].startswith("D "):
            i += 1
            continue
        _, seed, n = lines[i].split()
        seed, n = int(seed), int(n)
        want = [l[2:] for l in lines[i + 1:i + 2 + n]]
        i += 1 + len(want)
        sounds = []
        cpu, st = new_demo(seed, sounds)
        match_frame(cpu, (0, 0), 0x80018F24)  # sub-state 0: the init
        games += 1
        for k in range(n + 1):
            if k:
                match_frame(cpu, (0, 0), 0x80018F24)
            got = match_line(cpu, st)
            if got != want[k]:
                bad += 1
                print(f"game {games} seed {seed} frame {k} sub-state {cpu.h(0x800C34AA)}:")
                for d in match_diff(got, want[k]):
                    print("  ", d)
                break
        else:
            print(f"game {games}: {n} frames match, mode {cpu.h(0x80100684)}")
        sys.stdout.flush()
    print(games, "games", bad, "diverged")


def match_frame(cpu, held, handler=0x80015FCC):
    held, ports = held[:2], (held[2] if len(held) > 2 else 0)
    v0, v1 = ~held[0] & 0xFFFF, ~held[1] & 0xFFFF
    cpu.wr(0x800B1443, 1, v0 & 0xFF)
    cpu.wr(0x800B1442, 1, v0 >> 8)
    cpu.wr(0x800B1465, 1, v1 & 0xFF)
    cpu.wr(0x800B1464, 1, v1 >> 8)
    # Pad status bytes: last frame's (no change) unless the dump's third field flips a port
    # (bit 0 port 1, bit 1 port 2); the main loop saves them after the handler.
    s0 = cpu.rd(0x8006626C, 1) ^ (0xFF if ports & 1 else 0)
    s1 = cpu.rd(0x8006626D, 1) ^ (0xFF if ports & 2 else 0)
    cpu.wr(0x800B1440, 1, s0)
    cpu.wr(0x800B1462, 1, s1)
    cpu.call(0x80014F20)
    cpu.call(0x8003D830)  # D_80066AD0 drives the garbage divisor
    cpu.call(handler)
    cpu.wr(0x8006626C, 1, s0)
    cpu.wr(0x8006626D, 1, s1)


def match_line(cpu, st, special=True):
    h = cpu.h
    s = ""
    for p in range(2):
        inc = [h(0x801003B8 + 14 * p + 2 * k) for k in range(7)]
        pend = [h(0x801006A8 + 14 * p + 2 * k) for k in range(7)]
        s += "%s # [%s] [%s] %d %d %d ## " % (
            line(cpu, st, p), ", ".join(map(str, inc)), ", ".join(map(str, pend)),
            h(0x800C3132 + 0x30 * p) if special else -1, h(0x80100674 + 2 * p), cpu.w(0x80069658 + 4 * p))
    return s


def match_diff(a, b):
    out = []
    for p, (x, y) in enumerate(zip(a.split(" ## ")[:2], b.split(" ## ")[:2])):
        xa, xb = x.split(" # "), y.split(" # ")
        out += [f"p{p} {d}" for d in diff(xa[0], xb[0])]
        if xa[1] != xb[1]:
            out.append(f"p{p} garbage/special/bombs/score emu {xa[1]} rust {xb[1]}")
    return out[:10]


def diff(a, b):
    """Name the first differing fields."""
    pa, pb = a.split(" | ")[0].split(), b.split(" | ")[0].split()
    names = "state x y speed rot pivot sat rot_cd flip_cd move_delay move_anim fall_delay lock landings".split()
    out = [f"{n} {x}/{y}" for n, x, y in zip(names, pa, pb) if x != y]
    if a.split(" | ")[1:3] != b.split(" | ")[1:3]:
        out.append("queue/seed %s / %s" % (a.split(" | ")[1:3], b.split(" | ")[1:3]))
    def cells(x):
        parts = (x + " ").split(" | ")
        return dict(t.split(":", 1) for t in parts[3].split()) if len(parts) > 3 else {}
    ca, cb = cells(a), cells(b)
    for k in sorted(set(ca) | set(cb)):
        if ca.get(k) != cb.get(k):
            out.append(f"cell {k} emu {ca.get(k)} rust {cb.get(k)}")
    return out[:8]


def main_match(vs_cpu=False):
    lines = open(sys.argv[2]).read().split("\n")
    i = 0
    games = bad = 0
    tag = "C " if vs_cpu else "M "
    handler = 0x80016480 if vs_cpu else 0x80015FCC
    while i < len(lines):
        if not lines[i].startswith(tag):
            i += 1
            continue
        f = lines[i].split()
        seed, pads = int(f[1]), f[-1]
        pads = [tuple(int(y) for y in x.split("/")) for x in pads.rstrip(",").split(",")]
        want = [l[2:] for l in lines[i + 1:i + 2 + len(pads)]]
        i += 1 + len(want)
        sounds = []
        if vs_cpu:
            cpu, st = new_vs_cpu(seed, *map(int, f[2:6]), sounds)
        else:
            cpu, st = new_match(seed, tuple(map(int, f[2:4])), tuple(map(int, f[4:6])), sounds)
        match_frame(cpu, (0, 0), handler)  # sub-state 0: the init
        games += 1
        for n, held in enumerate([None] + pads):
            if held is not None:
                match_frame(cpu, held, handler)
            got = match_line(cpu, st)
            if got != want[n]:
                bad += 1
                print(f"game {games} seed {seed} frame {n} pad {held}:")
                for d in match_diff(got, want[n]):
                    print("  ", d)
                break
        else:
            end = f", ends in sub-state {cpu.h(0x800C34AA)}" if vs_cpu else ""
            print(f"game {games}: {len(pads)} frames match{end}")
        sys.stdout.flush()
    print(games, "games", bad, "diverged")


def pause_tail(cpu, sounds):
    """The sub-state, pause menu (`D_80103248`, cursor, timer), who paused and the SEs."""
    sub = cpu.h(0x800C34AA)
    return "%d %d %d %d %d [%s]" % (
        sub, cpu.w(0x80103248), (cpu.rd(0x800B1400, 1) ^ 0x80) - 0x80, cpu.w(0x8006850C),
        cpu.h(0x8010ADAE) if sub == 3 else -1, ", ".join(str(s) for s in sorted({s & 0xFF for s in sounds})))


def main_match_pause(vs_cpu=False, challenger=False):
    lines = open(sys.argv[2]).read().split("\n")
    tag, handler = ("Q ", 0x80016480) if vs_cpu else ("R " if challenger else "P ", 0x80015FCC)
    i = games = bad = 0
    while i < len(lines):
        if not lines[i].startswith(tag):
            i += 1
            continue
        f = lines[i].split()
        seed, pads = int(f[1]), f[-1]
        pads = [tuple(int(y) for y in x.split("/")) for x in pads.rstrip(",").split(",")]
        want = [l[2:] for l in lines[i + 1:i + 2 + len(pads)]]
        i += 1 + len(want)
        sounds = []
        if vs_cpu:
            cpu, st = new_vs_cpu(seed, *map(int, f[2:6]), sounds)
            # The mascot's voice lines come from its actor script (op func_800328FC), which the
            # port runs in the web layer (sdchr::Vm), not in VsCpu::sounds.
            se = cpu.hooks[SE]
            cpu.hooks[SE] = lambda c, se=se: c.r.__setitem__(2, 0) if c.r[31] == 0x80032924 else se(c)
        elif challenger:
            cpu, st = new_challenger(seed, int(f[2]), int(f[3]), (int(f[4]), int(f[5])), sounds)
        else:
            cpu, st = new_match(seed, tuple(map(int, f[2:4])), tuple(map(int, f[4:6])), sounds)
        # What the previous mode (13 or 15) left in RAM: each player's flip / move delays and
        # soft-drop points, which the init keeps.
        carry = f[-3:-1] if (vs_cpu and len(f) == 9) or (challenger and len(f) == 9) else []
        for p, k in enumerate(carry):
            flip, move, drop = map(int, k.split(","))
            cpu.seth(P + 0x50 * p + 0x16, flip)
            cpu.seth(P + 0x50 * p + 0x18, move)
            cpu.seth(0x80069524 + 2 * p, drop)
        match_frame(cpu, (0, 0), handler)  # sub-state 0: the init
        games += 1
        for n, held in enumerate([None] + pads):
            sounds.clear()
            if held is not None:
                try:
                    match_frame(cpu, held, handler)
                except Frozen:
                    print(f"game {games}: {n - 1} frames match, then the original freezes (func_8002F278)")
                    break
            body, tail = want[n].split("#P ")
            got_body, got_tail = match_line(cpu, st), pause_tail(cpu, sounds)
            if tail.startswith("-1 "):
                # Left through Quit: the mode and sub-state are the title's.
                ok = got_body == body
                got_tail = f"mode {cpu.h(0x80100684)} " + got_tail
            else:
                ok = got_body == body and got_tail == tail
            if not ok:
                bad += 1
                print(f"game {games} seed {seed} frame {n} pad {held}:")
                for d in match_diff(got_body, body):
                    print("  ", d)
                print("   emu  #", got_tail)
                print("   rust #", tail)
                break
        else:
            print(f"game {games}: {len(pads)} frames match, mode {cpu.h(0x80100684)} sub-state {cpu.h(0x800C34AA)}")
        sys.stdout.flush()
    print(games, "games", bad, "diverged")


SD_OBJECTS = 0x8005ECA4


def ending_line(cpu, xa, sounds):
    """The Rust `ending::tests::line`: state, timer, staff page, done, brightness, XA, SEs and
    the live SD objects (0x48 bytes: +0 x, +4 y, +8 layer, +0xC flip, +0x1C frame record,
    +0x24/+0x26 scale)."""
    h, w = cpu.h, cpu.w
    s = "%d %d %d %d |" % (h(0x80100410), h(0x8005F3B4), h(0x8005F3B8), int(h(0x80100684) != 16))
    s += "".join(" %d" % cpu.rd(0x80103610 + i, 1) for i in range(25))
    s += " | %d [%s] |" % (xa[-1] if xa else -1, ",".join(str(x & 0xFF) for x in dict.fromkeys(sounds)))
    for n in range(25):
        o = SD_OBJECTS + 0x48 * n
        layer = s32(w(o + 8))
        if layer < 0:
            continue
        f = w(o + 0x1C)
        fr = [h(f + 2 * i) for i in range(9)] if f else [0] * 9
        s += " %d:%d,%d,%d,%d,%d,%d/%s" % (n, layer, s32(w(o + 0xC)), h(o), h(o + 4), h(o + 0x24), h(o + 0x26),
                                           ",".join(map(str, fr)))
    return s


def main_ending():
    lines = open(sys.argv[2]).read().split("\n")
    i = games = bad = 0
    while i < len(lines):
        parts = lines[i].split()
        if len(parts) != 3 or parts[0] != "N":
            i += 1
            continue
        course, last = int(parts[1]), int(parts[2])
        want = []
        i += 1
        while i < len(lines) and lines[i].startswith("N ") and len(lines[i].split()) != 3:
            want.append(lines[i][2:])
            i += 1
        sounds, xa = [], []
        cpu, st = new_cpu(0, sounds)
        for a in MATCH_STUBS + [0x80030CF0, 0x80030F7C, 0x8002CE68]:
            cpu.hooks[a] = cpu.hooks[STUBS[0]]
        del cpu.hooks[0x8002EB38]

        def xa_hook(c):
            if c.r[5] == 1:  # (entry, 1) plays; (1, 2) stops
                xa.append(s32(c.r[4]))
            c.r[2] = 0
        cpu.hooks[0x80040A7C] = xa_hook
        sd = open(SD_CHR, "rb").read()
        cpu.mem[0x1A8000:0x1A8000 + len(sd)] = sd
        cpu.watch[0x8002F278] = frozen
        for n in range(25):
            cpu.setw(SD_OBJECTS + 0x48 * n + 8, -1)
        cpu.seth(0x80100684, 16)
        cpu.seth(0x80100410, 0)
        cpu.seth(0x800C30C8, course)
        cpu.seth(0x800C2FE0, last)
        games += 1
        for n, want_line in enumerate(want):
            sounds.clear()
            xa.clear()
            cpu.call(0x80030798)
            got = ending_line(cpu, xa, sounds)
            if got != want_line:
                bad += 1
                print(f"ending {course}/{last} frame {n}:")
                print("   emu ", got)
                print("   rust", want_line)
                break
        else:
            print(f"ending {course}/{last}: {len(want)} frames match, mode {cpu.h(0x80100684)}")
        sys.stdout.flush()
    print(games, "endings", bad, "diverged")


# Keep in step with stackers-web vs_cpu::actor_dump::GARBAGE_AT.
ACTOR_GARBAGE_AT = (60, 700, 1400)


def main_actor():
    lines = open(sys.argv[2]).read().split("\n")
    i = games = bad = 0
    while i < len(lines):
        if not lines[i].startswith("A "):
            i += 1
            continue
        f = lines[i].split()
        seed, course, stage, last = map(int, f[1:5])
        pads = [int(x) for x in f[5].split(",") if x]
        want = [l[2:] for l in lines[i + 1:i + 1 + len(pads)]]
        i += 1 + len(want)
        sounds = []
        cpu, st = new_vs_cpu(seed, 0, course, stage, last, sounds)
        actor = []
        se = cpu.hooks[SE]
        def hook(c, se=se):
            if c.r[31] == 0x80032924:
                actor.append(c.r[4] & 0xFF)
                c.r[2] = 0
            else:
                se(c)
        cpu.hooks[SE] = hook
        match_frame(cpu, (0, 0), 0x80016480)  # sub-state 0: the init
        games += 1
        for n, held in enumerate(pads):
            if n in ACTOR_GARBAGE_AT:
                for c in range(7):
                    cpu.seth(0x801006A8 + 14 + 2 * c, 6 if c else 0)
            actor.clear()
            match_frame(cpu, (held, 0), 0x80016480)
            got = "%d %d %d %s" % (cpu.h(0x8005F3C8), int(cpu.h(0x8005F3D8) != 0), cpu.h(0x80100640 + 2),
                                    "[" + ", ".join(map(str, sorted(set(actor)))) + "]")
            if got != want[n]:
                bad += 1
                print(f"game {games} seed {seed} frame {n}: emu {got} rust {want[n]}")
                break
        else:
            print(f"game {games}: {len(pads)} frames match")
        sys.stdout.flush()
    print(games, "games", bad, "diverged")


def main():
    if sys.argv[1] == "ending":
        return main_ending()
    if sys.argv[1] == "actor":
        return main_actor()
    if sys.argv[1] in ("matchpause", "cpupause", "challenger"):
        return main_match_pause(sys.argv[1] == "cpupause", sys.argv[1] == "challenger")
    if sys.argv[1] in ("end", "chainend"):
        return main_end(sys.argv[1] == "chainend")
    if sys.argv[1] == "chain":
        return main_chain()
    if sys.argv[1] == "trial":
        return main_trial()
    if sys.argv[1] == "trialend":
        return main_trial_end()
    if sys.argv[1] == "demo":
        return main_demo()
    if sys.argv[1] in ("match", "cpu"):
        return main_match(sys.argv[1] == "cpu")
    lines = open(sys.argv[1]).read().split("\n")
    i = 0
    games = bad = 0
    while i < len(lines):
        if not lines[i].startswith("G "):
            i += 1
            continue
        _, seed, pads = lines[i].split()
        seed = int(seed)
        pads = [int(x) for x in pads.rstrip(",").split(",")]
        want = [l[2:] for l in lines[i + 1:i + 2 + len(pads)]]
        i += 1 + len(want)
        sounds = []
        cpu, st = new_cpu(seed, sounds)
        frame(cpu, 0)  # sub-state 0: the init
        games += 1
        for n, held in enumerate([None] + pads):
            if held is not None:
                frame(cpu, held)
            got = line(cpu, st)
            if got != want[n]:
                bad += 1
                print(f"game {games} seed {seed} frame {n} pad {held}:")
                for d in diff(got, want[n]):
                    print("  ", d)
                break
    print(games, "games", bad, "diverged")


if __name__ == "__main__":
    main()
