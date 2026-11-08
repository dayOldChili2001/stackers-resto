#!/usr/bin/env python3
"""Extract the ISO9660 filesystem from a PS1 MODE2/2352 data track.

Usage: python3 -I tools/extract_disc.py <track01.bin> <out_dir>

Form 1 files (normal data) are written as plain 2048-byte-sector files.
Files containing any Form 2 sectors (XA audio / STR video) are written as
raw 2352-byte sectors with a `.raw2352` suffix appended, so nothing is lost.
"""
import os
import struct
import sys

RAW = 2352
USER_OFF = 24  # 12 sync + 4 header + 8 subheader
USER_LEN = 2048


class Disc:
    def __init__(self, path):
        self.f = open(path, "rb")
        self.f.seek(0, os.SEEK_END)
        self.nsectors = self.f.tell() // RAW

    def raw(self, lba):
        self.f.seek(lba * RAW)
        return self.f.read(RAW)

    def user(self, lba):
        return self.raw(lba)[USER_OFF:USER_OFF + USER_LEN]

    def is_form2(self, lba):
        return bool(self.raw(lba)[18] & 0x20)


def parse_dir(disc, lba, size, path, out):
    data = b"".join(disc.user(lba + i) for i in range((size + 2047) // 2048))
    pos = 0
    while pos < len(data):
        rec_len = data[pos]
        if rec_len == 0:
            pos = (pos // 2048 + 1) * 2048  # skip to next sector
            continue
        rec = data[pos:pos + rec_len]
        ext_lba = struct.unpack_from("<I", rec, 2)[0]
        ext_size = struct.unpack_from("<I", rec, 10)[0]
        flags = rec[25]
        name_len = rec[32]
        name = rec[33:33 + name_len]
        pos += rec_len
        if name in (b"\x00", b"\x01"):
            continue
        name = name.decode("ascii", "replace").split(";")[0]
        full = f"{path}/{name}" if path else name
        if flags & 0x02:
            parse_dir(disc, ext_lba, ext_size, full, out)
        else:
            out.append((full, ext_lba, ext_size))


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    disc = Disc(sys.argv[1])
    out_dir = sys.argv[2]
    pvd = disc.user(16)
    if pvd[1:6] != b"CD001":
        sys.exit("No ISO9660 primary volume descriptor at sector 16")
    root = pvd[156:156 + 34]
    root_lba = struct.unpack_from("<I", root, 2)[0]
    root_size = struct.unpack_from("<I", root, 10)[0]

    files = []
    parse_dir(disc, root_lba, root_size, "", files)
    for name, lba, size in sorted(files, key=lambda f: f[1]):
        nsec = (size + 2047) // 2048
        if lba >= disc.nsectors:
            # Directory entry pointing into a CD-DA audio track (e.g. *.DA): not data.
            print(f"{lba:7d} {size:10d} DA  {name} (CD audio track, skipped)")
            continue
        form2 = any(disc.is_form2(lba + i) for i in range(nsec) if lba + i < disc.nsectors)
        dest = os.path.join(out_dir, name + (".raw2352" if form2 else ""))
        os.makedirs(os.path.dirname(dest) or out_dir, exist_ok=True)
        with open(dest, "wb") as w:
            if form2:
                # Form 2 sectors: the directory size counts 2048/sector, so the sector count still holds.
                for i in range(nsec):
                    w.write(disc.raw(lba + i))
            else:
                buf = b"".join(disc.user(lba + i) for i in range(nsec))
                w.write(buf[:size])
        print(f"{lba:7d} {size:10d} {'XA ' if form2 else '   '}{name}")
    print(f"{len(files)} files -> {out_dir}")


if __name__ == "__main__":
    main()
