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
    cpu.setw(0x80069658, 0)
    cpu.setw(0x8006965C, 0)
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
    v0, v1 = ~held[0] & 0xFFFF, ~held[1] & 0xFFFF
    cpu.wr(0x800B1443, 1, v0 & 0xFF)
    cpu.wr(0x800B1442, 1, v0 >> 8)
    cpu.wr(0x800B1465, 1, v1 & 0xFF)
    cpu.wr(0x800B1464, 1, v1 >> 8)
    # Pad status bytes as the plugged-in check expects (no unplug pause).
    cpu.wr(0x800B1440, 1, cpu.rd(0x8006626C, 1))
    cpu.wr(0x800B1462, 1, cpu.rd(0x8006626D, 1))
    cpu.call(0x80014F20)
    cpu.call(0x8003D830)  # D_80066AD0 drives the garbage divisor
    cpu.call(handler)


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


def main():
    if sys.argv[1] == "chain":
        return main_chain()
    if sys.argv[1] == "trial":
        return main_trial()
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
