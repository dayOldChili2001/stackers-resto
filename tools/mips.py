"""Minimal R3000 integer interpreter for running SLUS_003.15 functions in isolation.

Loads the user's own executable (extracted/disc/SLUS_003.15, see extract_disc.py) and runs a
function with chosen arguments and memory, for differential tests of the Rust port. No FPU, GTE,
interrupts or BIOS: BIOS calls (e.g. rand at 0x800524A4) must be replaced via `Cpu.hooks`.
"""
import os
import struct

EXE = os.path.join(os.path.dirname(__file__), "..", "extracted", "disc", "SLUS_003.15")
RAM = 0x200000
SENTINEL = 0x80000100


def s16(x):
    x &= 0xFFFF
    return x - 0x10000 if x & 0x8000 else x


def s32(x):
    x &= 0xFFFFFFFF
    return x - 0x100000000 if x & 0x80000000 else x


class Cpu:
    def __init__(self):
        d = open(EXE, "rb").read()
        self.mem = bytearray(RAM)
        t_addr, t_size = struct.unpack("<II", d[0x18:0x20])
        o = t_addr & 0x1FFFFF
        self.mem[o:o + t_size] = d[0x800:0x800 + t_size]
        self.r = [0] * 32
        self.hi = self.lo = 0
        self.on_read = None  # (addr, size) -> None
        self.on_write = None
        self.on_call = None
        self.watch = {}
        self.hooks = {}  # pc -> fn(cpu) replacing the function (must set v0)
        self.steps = 0

    # memory
    def a(self, addr):
        return addr & 0x1FFFFF

    def rd(self, addr, n):
        if self.on_read:
            self.on_read(addr & 0xFFFFFFFF, n)
        o = self.a(addr)
        return int.from_bytes(self.mem[o:o + n], "little")

    def wr(self, addr, n, v):
        if self.on_write:
            self.on_write(addr & 0xFFFFFFFF, n, v)
        o = self.a(addr)
        self.mem[o:o + n] = (v & ((1 << (8 * n)) - 1)).to_bytes(n, "little")

    def h(self, addr):
        return s16(self.rd(addr, 2))

    def seth(self, addr, v):
        o = self.a(addr)
        self.mem[o:o + 2] = (v & 0xFFFF).to_bytes(2, "little")

    def setw(self, addr, v):
        o = self.a(addr)
        self.mem[o:o + 4] = (v & 0xFFFFFFFF).to_bytes(4, "little")

    def w(self, addr):
        o = self.a(addr)
        return int.from_bytes(self.mem[o:o + 4], "little")

    def call(self, pc, *args, limit=50_000_000):
        r = self.r
        for i, v in enumerate(args):
            r[4 + i] = v & 0xFFFFFFFF
        r[31] = SENTINEL
        r[29] = 0x801FFF00
        self.steps = 0
        self._run(pc, limit)
        return s32(r[2])

    def _run(self, pc, limit):
        r = self.r
        npc = pc + 4
        mem = self.mem
        while True:
            if pc == SENTINEL:
                return
            if pc in self.watch:
                self.watch[pc](self)
            if pc in self.hooks:
                self.hooks[pc](self)
                pc = r[31]
                npc = pc + 4
                continue
            self.steps += 1
            if self.steps > limit:
                raise RuntimeError("step limit")
            o = pc & 0x1FFFFF
            ins = mem[o] | mem[o + 1] << 8 | mem[o + 2] << 16 | mem[o + 3] << 24
            op = ins >> 26
            rs = (ins >> 21) & 31
            rt = (ins >> 16) & 31
            rd = (ins >> 11) & 31
            sa = (ins >> 6) & 31
            imm = ins & 0xFFFF
            simm = imm - 0x10000 if imm & 0x8000 else imm
            target = None
            cur = pc
            pc = npc
            npc = pc + 4
            if op == 0:
                fn = ins & 63
                if fn == 0x00: v = r[rt] << sa
                elif fn == 0x02: v = r[rt] >> sa
                elif fn == 0x03: v = s32(r[rt]) >> sa
                elif fn == 0x04: v = r[rt] << (r[rs] & 31)
                elif fn == 0x06: v = r[rt] >> (r[rs] & 31)
                elif fn == 0x07: v = s32(r[rt]) >> (r[rs] & 31)
                elif fn == 0x08:
                    npc = r[rs]; v = None
                elif fn == 0x09:
                    v = None; ret = pc + 4; npc = r[rs]
                    if rd: r[rd] = ret
                elif fn == 0x10: v = self.hi
                elif fn == 0x12: v = self.lo
                elif fn == 0x11: self.hi = r[rs]; v = None
                elif fn == 0x13: self.lo = r[rs]; v = None
                elif fn == 0x18:
                    p = s32(r[rs]) * s32(r[rt]); self.lo = p & 0xFFFFFFFF; self.hi = (p >> 32) & 0xFFFFFFFF; v = None
                elif fn == 0x19:
                    p = r[rs] * r[rt]; self.lo = p & 0xFFFFFFFF; self.hi = (p >> 32) & 0xFFFFFFFF; v = None
                elif fn == 0x1A:
                    a_, b_ = s32(r[rs]), s32(r[rt])
                    if b_ == 0:
                        self.lo = 0xFFFFFFFF if a_ >= 0 else 1; self.hi = a_
                    else:
                        q = abs(a_) // abs(b_)
                        if (a_ < 0) != (b_ < 0): q = -q
                        self.lo = q & 0xFFFFFFFF; self.hi = (a_ - q * b_) & 0xFFFFFFFF
                    v = None
                elif fn == 0x1B:
                    if r[rt] == 0:
                        self.lo = 0xFFFFFFFF; self.hi = r[rs]
                    else:
                        self.lo = r[rs] // r[rt]; self.hi = r[rs] % r[rt]
                    v = None
                elif fn in (0x20, 0x21): v = r[rs] + r[rt]
                elif fn in (0x22, 0x23): v = r[rs] - r[rt]
                elif fn == 0x24: v = r[rs] & r[rt]
                elif fn == 0x25: v = r[rs] | r[rt]
                elif fn == 0x26: v = r[rs] ^ r[rt]
                elif fn == 0x27: v = ~(r[rs] | r[rt])
                elif fn == 0x2A: v = int(s32(r[rs]) < s32(r[rt]))
                elif fn == 0x2B: v = int(r[rs] < r[rt])
                else:
                    raise RuntimeError(f"special {fn:#x} at {cur:#x}")
                if v is not None and rd:
                    r[rd] = v & 0xFFFFFFFF
                continue
            if op == 1:
                a_ = s32(r[rs])
                if rt in (0x10, 0x11):
                    r[31] = pc + 4
                cond = a_ < 0 if rt & 1 == 0 else a_ >= 0
                if cond:
                    npc = (pc + (simm << 2)) & 0xFFFFFFFF
                continue
            if op in (2, 3):
                if op == 3:
                    r[31] = pc + 4
                npc = (pc & 0xF0000000) | ((ins & 0x3FFFFFF) << 2)
                if op == 3 and self.on_call:
                    self.on_call(cur, npc)
                continue
            if 4 <= op <= 7:
                a_, b_ = s32(r[rs]), s32(r[rt])
                cond = [a_ == b_, a_ != b_, a_ <= 0, a_ > 0][op - 4]
                if cond:
                    npc = (pc + (simm << 2)) & 0xFFFFFFFF
                continue
            if op in (8, 9): v = r[rs] + simm
            elif op == 0xA: v = int(s32(r[rs]) < simm)
            elif op == 0xB: v = int(r[rs] < (simm & 0xFFFFFFFF))
            elif op == 0xC: v = r[rs] & imm
            elif op == 0xD: v = r[rs] | imm
            elif op == 0xE: v = r[rs] ^ imm
            elif op == 0xF: v = imm << 16
            elif op >= 0x20:
                addr = (r[rs] + simm) & 0xFFFFFFFF
                if op == 0x20: v = self.rd(addr, 1); v = v - 256 if v & 0x80 else v
                elif op == 0x24: v = self.rd(addr, 1)
                elif op == 0x21: v = s16(self.rd(addr, 2))
                elif op == 0x25: v = self.rd(addr, 2)
                elif op == 0x23: v = self.rd(addr, 4)
                elif op == 0x22:  # lwl
                    al = addr & ~3; sh = addr & 3; w = self.rd(al, 4)
                    m = (0x00FFFFFF, 0x0000FFFF, 0x000000FF, 0)[sh]
                    v = (r[rt] & m) | (w << (24 - 8 * sh))
                elif op == 0x26:  # lwr
                    al = addr & ~3; sh = addr & 3; w = self.rd(al, 4)
                    m = (0, 0xFF000000, 0xFFFF0000, 0xFFFFFF00)[sh]
                    v = (r[rt] & m) | (w >> (8 * sh))
                elif op == 0x28: self.wr(addr, 1, r[rt]); continue
                elif op == 0x29: self.wr(addr, 2, r[rt]); continue
                elif op == 0x2B: self.wr(addr, 4, r[rt]); continue
                elif op == 0x2A:  # swl
                    al = addr & ~3; sh = addr & 3; w = self.rd(al, 4)
                    m = (0xFFFFFF00, 0xFFFF0000, 0xFF000000, 0)[sh]
                    self.wr(al, 4, (w & m) | (r[rt] >> (24 - 8 * sh))); continue
                elif op == 0x2E:  # swr
                    al = addr & ~3; sh = addr & 3; w = self.rd(al, 4)
                    m = (0, 0xFF, 0xFFFF, 0xFFFFFF)[sh]
                    self.wr(al, 4, (w & m) | (r[rt] << (8 * sh))); continue
                else:
                    raise RuntimeError(f"op {op:#x} at {cur:#x}")
            else:
                raise RuntimeError(f"op {op:#x} at {cur:#x}")
            if rt:
                r[rt] = v & 0xFFFFFFFF


FIELD = 0x800B1A40


def cell(p, c, row):
    return FIELD + p * 0x8A40 + c * 0x1148 + row * 0x38


def decode_cell(addr):
    o = addr - FIELD
    p, o = divmod(o, 0x8A40)
    c, o = divmod(o, 0x1148)
    row, f = divmod(o, 0x38)
    return p, c, row, f
