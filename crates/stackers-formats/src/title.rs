//! Title and mode-menu art: sprite records in the executable, the texture pages the title
//! loads (`func_800174D4`), and the tiled background map in `DATA/DOMI_BG.OBJ`.

use crate::anm::Frame;
use crate::exe::Exe;
use crate::{u16le, u32le, Error};

/// TECMO logo (TROGO, page 8).
const TECMO: u32 = 0x8005_6E84;
/// Title logo as one 240×224 sprite (ROGO_FF, page 3), used for the spin-in.
const LOGO: u32 = 0x8005_5C78;
/// Menu arrow animation, 33 records (MODE, page 4).
const ARROWS: u32 = 0x8005_5C8C;
const ARROW_COUNT: usize = 33;
/// Mode descriptions (page 26 = VNAO3), one per menu item.
const DESCRIPTIONS: u32 = 0x8005_6C70;
/// Arcade set-match counts "1", "3", "5" (LEVEL, page 5), `func_8002855C`.
const SET_DIGITS: u32 = 0x8005_6A18;
/// The □ stock label of `func_8001D778` (MSG2, page 22): normal, and while player state 12.
const STOCK_LABELS: u32 = 0x8005_6CE0;
/// `func_8001D4D8` font 2 (PSFONT, page 12): 8×8 digits.
const SMALL_DIGITS: u32 = 0x8005_65B8;

/// Texture pages are numbered by `D_80066278` slot. The title reuses slots the solo screen
/// fills with other files, so the page ids handed to the renderer for title art are
/// `PAGE_BASE + slot`, and `VNAO1–3` (which share page 26/27's VRAM) get their own ids.
pub const PAGE_BASE: i32 = 100;
/// `PAPER.TIM`: the background tiles' texture (tpage `0x401F`, not one of the numbered slots).
pub const PAPER_PAGE: i32 = PAGE_BASE + 40;
/// Renderer page id of `VNAOn.TIM` (n = 1–3).
pub fn vnao_page(n: u8) -> i32 {
    PAGE_BASE + 40 + i32::from(n)
}

/// Disc file behind a title page id (see [`PAGE_BASE`]).
pub fn page_file(page: i32) -> Option<&'static str> {
    Some(match page - PAGE_BASE {
        3 => "DERODERO/GRAPH/OPENING/ROGO_FF.TIM",
        4 => "DERODERO/GRAPH/OPENING/MODE.TIM",
        5 => "DERODERO/GRAPH/OPENING/LEVEL.TIM",
        6 => "DERODERO/GRAPH/OPENING/KABEE2A.TIM",
        7 => "DERODERO/GRAPH/OPENING/KABEE2B.TIM",
        8 => "DERODERO/GRAPH/OPENING/TROGO.TIM",
        // PAPER is uploaded into slot 12 first, then PSFONT replaces the slot's tpage.
        12 => "DERODERO/GRAPH/PSFONT.TIM",
        14 => "DERODERO/GRAPH/OPENING/OPTION.TIM",
        22 => "DERODERO/GRAPH/MSG2.TIM",
        40 => "DERODERO/GRAPH/DOMINO/PAPER.TIM",
        41 => "DERODERO/GRAPH/VNAO1.TIM",
        42 => "DERODERO/GRAPH/VNAO2.TIM",
        43 => "DERODERO/GRAPH/VNAO3.TIM",
        _ => return None,
    })
}

/// Every title page id.
pub fn pages() -> Vec<i32> {
    (PAGE_BASE..PAGE_BASE + 44).filter(|&p| page_file(p).is_some()).collect()
}

pub const BG_FILE: &str = "DERODERO/DATA/DOMI_BG.OBJ";

#[derive(Clone, Debug)]
pub struct TitleLayout {
    pub tecmo: Frame,
    pub logo: Frame,
    pub arrows: Vec<Frame>,
    pub descriptions: Vec<Frame>,
    pub set_digits: Vec<Frame>,
    pub stock_labels: Vec<Frame>,
    pub small_digits: Vec<Frame>,
}

impl TitleLayout {
    pub fn from_exe(exe: &[u8]) -> Result<Self, Error> {
        let exe = Exe::parse(exe)?;
        let frame = |addr: u32| Frame::from_bytes(exe.at(addr, 18)?).ok_or(Error::Truncated);
        let list = |addr: u32, n: usize| (0..n).map(|i| frame(addr + 18 * i as u32)).collect::<Result<Vec<_>, _>>();
        let layout = TitleLayout {
            tecmo: frame(TECMO)?,
            logo: frame(LOGO)?,
            arrows: list(ARROWS, ARROW_COUNT)?,
            descriptions: list(DESCRIPTIONS, 5)?,
            set_digits: list(SET_DIGITS, 3)?,
            stock_labels: list(STOCK_LABELS, 2)?,
            small_digits: list(SMALL_DIGITS, 10)?,
        };
        if layout.tecmo.page != 8 || layout.logo.page != 3 || layout.descriptions.iter().any(|f| f.page != 26) {
            return Err(Error::BadMagic("title sprite records"));
        }
        Ok(layout)
    }
}

/// One GsBG layer of `DOMI_BG.OBJ` (set up by `func_8002430C` → `func_800243E8`): a map of
/// `cols × rows` cells, each `cell_w × cell_h` texels taken from one 4-bit texture page.
///
/// File layout (all pointers absolute, the file is loaded at `0x80160000`): word *n* points to
/// set *n* = `u32 count` then per layer 16 bytes `(u16 layer, u16 mode, u32 data, u16 attr,
/// u16 ?, u16 ?, 2 pad)`. `data` → `(u32 map, u32 cells)`; `map` → `+4 cols, +5 rows,
/// +6 cell_w, +7 cell_h`, then u16 cell indices from `+8`; `cells` → GsCELL records (u, v,
/// u16 clut, u16 flag, u16 tpage) from `+8`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BgMap {
    pub cols: usize,
    pub rows: usize,
    pub cell_w: i32,
    pub cell_h: i32,
    /// Texel (u, v) of each map position, row-major.
    pub tiles: Vec<(i32, i32)>,
}

const BG_BASE: u32 = 0x8016_0000;

impl BgMap {
    /// Layer 0 of set `set`.
    pub fn parse(data: &[u8], set: usize) -> Result<Self, Error> {
        let at = |addr: u32| -> Result<usize, Error> {
            addr.checked_sub(BG_BASE).map(|o| o as usize).filter(|&o| o < data.len()).ok_or(Error::Truncated)
        };
        let set_ptr = u32le(data, 4 * set)?;
        let s = at(set_ptr)?;
        if u32le(data, s)? == 0 {
            return Err(Error::BadMagic("DOMI_BG layer"));
        }
        let d = at(u32le(data, s + 8)?)?;
        let map = at(u32le(data, d)?)?;
        let cells = at(u32le(data, d + 4)?)?;
        let byte = |o: usize| data.get(o).copied().ok_or(Error::Truncated);
        let (cols, rows) = (usize::from(byte(map + 4)?), usize::from(byte(map + 5)?));
        let (cell_w, cell_h) = (i32::from(byte(map + 6)?), i32::from(byte(map + 7)?));
        let tiles = (0..cols * rows)
            .map(|i| {
                let idx = usize::from(u16le(data, map + 8 + 2 * i)?);
                let c = cells + 8 + 8 * idx;
                Ok((i32::from(byte(c)?), i32::from(byte(c + 1)?)))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(BgMap { cols, rows, cell_w, cell_h, tiles })
    }

    /// Screen-space tiles `(u, v, x, y)` covering 320×240 at GsBG scroll `(sx, sy)`
    /// (the map repeats).
    pub fn tiles_at(&self, scroll: (i16, i16)) -> Vec<(i32, i32, i32, i32)> {
        let (w, h) = (self.cols as i32 * self.cell_w, self.rows as i32 * self.cell_h);
        if w == 0 || h == 0 {
            return Vec::new();
        }
        let ox = i32::from(scroll.0).rem_euclid(w);
        let oy = i32::from(scroll.1).rem_euclid(h);
        let mut out = Vec::new();
        let mut y = -(oy % self.cell_h);
        let mut row = oy / self.cell_h;
        while y < 240 {
            let mut x = -(ox % self.cell_w);
            let mut col = ox / self.cell_w;
            while x < 320 {
                let (u, v) = self.tiles[(row as usize % self.rows) * self.cols + col as usize % self.cols];
                out.push((u, v, x, y));
                x += self.cell_w;
                col += 1;
            }
            y += self.cell_h;
            row += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolled_tiles_wrap_and_cover_the_screen() {
        let map = BgMap { cols: 2, rows: 1, cell_w: 64, cell_h: 64, tiles: vec![(0, 0), (64, 0)] };
        let t = map.tiles_at((70, -10));
        // x: 70 % 128 = 70 → column 1 shifted by 6; y: −10 → 118 % 64 = 54 into row 0 (wraps).
        assert_eq!(t[0], (64, 0, -6, -54));
        assert_eq!(t[1], (0, 0, 58, -54));
        assert!(t.iter().all(|&(_, _, x, y)| x > -64 && y > -64 && x < 320 && y < 240));
        assert_eq!(t.len(), 6 * 5);
    }
}
