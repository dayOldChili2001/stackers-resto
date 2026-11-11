"""Differential check of a whole solo game (mode 4) against the original, frame by frame.

    FRAME_DUMP=/tmp/f.txt cargo test -p stackers-core game::tests::frame_dump -- --ignored
    python3 tools/framediff.py /tmp/f.txt

Each game is replayed from the same BIOS rand seed and pad sequence through the original pad
reader `func_80014F20` and mode handler `func_80017098` (drawing stubbed out), and the player,
queue, rand seed and cells are compared after every frame. Reports the first divergence per game.
"""
import os
import sys
sys.path.insert(0, os.path.dirname(__file__))
from mips import *

ANM = os.path.join(os.path.dirname(__file__), "..", "extracted", "disc", "DERODERO", "DATA", "BLT_ANM.OBJ")
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
    cpu.call(0x80017098)


def line(cpu, st):
    h = cpu.h
    s = "%d %d %d %d %d %d %d %d %d %d %d %d %d %d |" % (
        h(P + 0xC), s32(cpu.w(P)), s32(cpu.w(P + 4)), cpu.w(P + 8), h(P + 0x12), h(P + 0xE),
        h(P + 0x10), h(P + 0x14), h(P + 0x16), h(P + 0x18), h(P + 0x1A), h(P + 0x1C),
        h(P + 0x1E), h(P + 0x20))
    for k in range(3):
        q = 0x800699F0 + 0x30 * k
        s += " %d,%d" % (h(q + 0x18), h(q))
    s += " | %d |" % st["seed"]
    for c in range(1, 7):
        for r_ in range(18):
            a = cell(0, c, r_)
            state, color = h(a), h(a + 2)
            if state != 0 or color != 0x80:
                s += " %d,%d:%d:%d:%d:%d:%d:%d:%d:%d:%d:%d" % (
                    c, r_, state, color, h(a + 4), h(a + 8), s32(cpu.w(a + 0xC)), s32(cpu.w(a + 0x10)),
                    h(a + 0x2C), int(h(a + 0x2A) == -1), h(a + 0x16), int(h(a + 0x28) != 0 and state > 2))
    return s


def diff(a, b):
    """Name the first differing fields."""
    pa, pb = a.split(" | ")[0].split(), b.split(" | ")[0].split()
    names = "state x y speed rot pivot sat rot_cd flip_cd move_delay move_anim fall_delay lock landings".split()
    out = [f"{n} {x}/{y}" for n, x, y in zip(names, pa, pb) if x != y]
    if a.split(" | ")[1:3] != b.split(" | ")[1:3]:
        out.append("queue/seed %s / %s" % (a.split(" | ")[1:3], b.split(" | ")[1:3]))
    ca = dict(t.split(":", 1) for t in a.split(" | ")[3].split())
    cb = dict(t.split(":", 1) for t in b.split(" | ")[3].split()) if len(b.split(" | ")) > 3 else {}
    for k in sorted(set(ca) | set(cb)):
        if ca.get(k) != cb.get(k):
            out.append(f"cell {k} emu {ca.get(k)} rust {cb.get(k)}")
    return out[:8]


def main():
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
