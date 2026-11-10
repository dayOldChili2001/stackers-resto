//! ISO9660 reader for a PS1 MODE2/2352 data track (the Track 01 `.bin`).

use crate::{u32le, Error};

const RAW: usize = 2352;
const USER_OFF: usize = 24; // 12 sync + 4 header + 8 subheader
const USER_LEN: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Full path with `/` separators, no `;1` version suffix, e.g. `DERODERO/GRAPH/ESRB.TIM`.
    pub path: String,
    pub lba: u32,
    pub size: u32,
}

impl Entry {
    /// True if the entry points past the end of the data track, i.e. into a CD audio track.
    pub fn is_cdda(&self, disc: &Disc) -> bool {
        self.lba as usize >= disc.sectors()
    }
}

pub struct Disc<'a> {
    raw: &'a [u8],
    entries: Vec<Entry>,
}

impl<'a> Disc<'a> {
    pub fn open(raw: &'a [u8]) -> Result<Self, Error> {
        let mut disc = Disc { raw, entries: Vec::new() };
        let pvd = disc.user(16)?;
        if &pvd[1..6] != b"CD001" {
            return Err(Error::BadMagic("ISO9660 primary volume descriptor"));
        }
        let root_lba = u32le(pvd, 156 + 2)?;
        let root_size = u32le(pvd, 156 + 10)?;
        let mut entries = Vec::new();
        disc.walk(root_lba, root_size, "", &mut entries, 0)?;
        entries.sort_by_key(|e| e.lba);
        disc.entries = entries;
        Ok(disc)
    }

    pub fn sectors(&self) -> usize {
        self.raw.len() / RAW
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    fn user(&self, lba: usize) -> Result<&'a [u8], Error> {
        let start = lba * RAW + USER_OFF;
        self.raw.get(start..start + USER_LEN).ok_or(Error::Truncated)
    }

    fn walk(&self, lba: u32, size: u32, prefix: &str, out: &mut Vec<Entry>, depth: u32) -> Result<(), Error> {
        if depth > 16 {
            return Err(Error::Unsupported("directory nesting too deep"));
        }
        let nsec = (size as usize).div_ceil(USER_LEN);
        for s in 0..nsec {
            let sec = self.user(lba as usize + s)?;
            let mut pos = 0;
            while pos < USER_LEN {
                let len = sec[pos] as usize;
                if len == 0 || pos + len > USER_LEN {
                    break; // rest of sector is padding
                }
                let rec = &sec[pos..pos + len];
                pos += len;
                let name_len = *rec.get(32).ok_or(Error::Truncated)? as usize;
                let name = rec.get(33..33 + name_len).ok_or(Error::Truncated)?;
                if name == [0] || name == [1] {
                    continue; // "." and ".."
                }
                let name = String::from_utf8_lossy(name);
                let name = name.split(';').next().unwrap_or_default();
                let path = if prefix.is_empty() { name.to_string() } else { format!("{prefix}/{name}") };
                let e_lba = u32le(rec, 2)?;
                let e_size = u32le(rec, 10)?;
                if rec[25] & 0x02 != 0 {
                    self.walk(e_lba, e_size, &path, out, depth + 1)?;
                } else {
                    out.push(Entry { path, lba: e_lba, size: e_size });
                }
            }
        }
        Ok(())
    }

    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(path))
    }

    /// Read a Form 1 file's contents (2048-byte user data per sector).
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>, Error> {
        if entry.is_cdda(self) {
            return Err(Error::Unsupported("file is a CD audio track"));
        }
        let nsec = (entry.size as usize).div_ceil(USER_LEN);
        let mut out = Vec::with_capacity(nsec * USER_LEN);
        for i in 0..nsec {
            out.extend_from_slice(self.user(entry.lba as usize + i)?);
        }
        out.truncate(entry.size as usize);
        Ok(out)
    }

    /// A file's whole raw sectors (2352 bytes each, sync and subheader included), for
    /// Form 2 / XA files whose `size` counts 2048 bytes per sector.
    pub fn raw_sectors(&self, entry: &Entry) -> Result<&'a [u8], Error> {
        if entry.is_cdda(self) {
            return Err(Error::Unsupported("file is a CD audio track"));
        }
        let nsec = (entry.size as usize).div_ceil(USER_LEN);
        let start = entry.lba as usize * RAW;
        self.raw.get(start..start + nsec * RAW).ok_or(Error::Truncated)
    }

    pub fn read_path(&self, path: &str) -> Result<Vec<u8>, Error> {
        self.read(self.find(path).ok_or(Error::NotFound)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal 2352-byte-sector image: PVD at 16, root dir at 18 with one file at 20.
    fn tiny_disc() -> Vec<u8> {
        fn user(img: &mut [u8], lba: usize) -> &mut [u8] {
            let s = lba * RAW + USER_OFF;
            &mut img[s..s + USER_LEN]
        }
        let mut img = vec![0u8; RAW * 21];
        let rec = |name: &[u8], lba: u32, size: u32, dir: bool| -> Vec<u8> {
            let mut r = vec![0u8; 33 + name.len() + (1 - name.len() % 2)];
            r[0] = r.len() as u8;
            r[2..6].copy_from_slice(&lba.to_le_bytes());
            r[10..14].copy_from_slice(&size.to_le_bytes());
            r[25] = if dir { 2 } else { 0 };
            r[32] = name.len() as u8;
            r[33..33 + name.len()].copy_from_slice(name);
            r
        };
        let pvd = user(&mut img, 16);
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        let root = rec(&[0], 18, 2048, true);
        pvd[156..156 + root.len()].copy_from_slice(&root);
        let mut dir = Vec::new();
        dir.extend(rec(&[0], 18, 2048, true));
        dir.extend(rec(&[1], 18, 2048, true));
        dir.extend(rec(b"HELLO.TXT;1", 20, 5, false));
        user(&mut img, 18)[..dir.len()].copy_from_slice(&dir);
        user(&mut img, 20)[..5].copy_from_slice(b"hello");
        img
    }

    #[test]
    fn reads_tiny_image() {
        let img = tiny_disc();
        let disc = Disc::open(&img).unwrap();
        assert_eq!(disc.entries().len(), 1);
        assert_eq!(disc.entries()[0].path, "HELLO.TXT");
        assert_eq!(disc.read_path("hello.txt").unwrap(), b"hello");
    }

    #[test]
    fn rejects_non_iso() {
        let img = vec![0u8; RAW * 20];
        assert!(matches!(Disc::open(&img), Err(Error::BadMagic(_))));
    }
}
