//! PlayStation TIM image decoder (4/8/16/24 bpp), output as RGBA8.

use crate::{u16le, u32le, Error};

#[derive(Debug, Clone)]
pub struct Tim {
    pub bpp: u8,
    /// Palettes (CLUT rows), each as 15-bit PS1 colours.
    pub cluts: Vec<Vec<u16>>,
    /// VRAM placement, useful later for sprite atlases.
    pub vram_x: u16,
    pub vram_y: u16,
    /// Width in pixels (not 16-bit VRAM units).
    pub width: u32,
    pub height: u32,
    pixels: Vec<u8>,
}

/// Convert a PS1 15-bit colour to RGBA8. 0x0000 is fully transparent, as on hardware.
pub fn color15(c: u16) -> [u8; 4] {
    let expand = |v: u16| ((v << 3) | (v >> 2)) as u8;
    let (r, g, b) = (c & 31, (c >> 5) & 31, (c >> 10) & 31);
    let a = if c == 0 { 0 } else { 255 };
    [expand(r), expand(g), expand(b), a]
}

impl Tim {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        if u32le(data, 0)? != 0x10 {
            return Err(Error::BadMagic("TIM"));
        }
        let flags = u32le(data, 4)?;
        let bpp = match flags & 7 {
            0 => 4,
            1 => 8,
            2 => 16,
            3 => 24,
            _ => return Err(Error::Unsupported("TIM mixed mode")),
        };
        let mut pos = 8;
        let mut cluts = Vec::new();
        if flags & 8 != 0 {
            let len = u32le(data, pos)? as usize;
            let w = u16le(data, pos + 8)? as usize;
            let h = u16le(data, pos + 10)? as usize;
            for row in 0..h {
                let base = pos + 12 + row * w * 2;
                cluts.push((0..w).map(|i| u16le(data, base + i * 2)).collect::<Result<_, _>>()?);
            }
            pos += len;
        }
        let len = u32le(data, pos)? as usize;
        let vram_x = u16le(data, pos + 4)?;
        let vram_y = u16le(data, pos + 6)?;
        let w16 = u16le(data, pos + 8)? as u32;
        let height = u16le(data, pos + 10)? as u32;
        let width = match bpp {
            4 => w16 * 4,
            8 => w16 * 2,
            16 => w16,
            _ => w16 * 2 / 3,
        };
        let px_len = (w16 * 2 * height) as usize;
        if len < 12 + px_len {
            return Err(Error::Truncated);
        }
        let pixels = data.get(pos + 12..pos + 12 + px_len).ok_or(Error::Truncated)?.to_vec();
        if bpp <= 8 && cluts.is_empty() {
            return Err(Error::Unsupported("paletted TIM without CLUT"));
        }
        Ok(Tim { bpp, cluts, vram_x, vram_y, width, height, pixels })
    }

    /// Decode to RGBA8 using CLUT row `clut` (ignored for 16/24 bpp).
    pub fn to_rgba(&self, clut: usize) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut out = Vec::with_capacity(w * h * 4);
        let pal = self.cluts.get(clut).or(self.cluts.first());
        let stride = self.pixels.len() / h.max(1);
        for y in 0..h {
            let row = &self.pixels[y * stride..(y + 1) * stride];
            for x in 0..w {
                let px = match self.bpp {
                    4 => {
                        let idx = (row[x / 2] >> ((x & 1) * 4)) & 0xF;
                        color15(pal.and_then(|p| p.get(idx as usize)).copied().unwrap_or(0))
                    }
                    8 => color15(pal.and_then(|p| p.get(row[x] as usize)).copied().unwrap_or(0)),
                    16 => color15(u16::from_le_bytes([row[x * 2], row[x * 2 + 1]])),
                    _ => [row[x * 3], row[x * 3 + 1], row[x * 3 + 2], 255],
                };
                out.extend_from_slice(&px);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(bpp_flag: u32, clut: Option<&[u16]>, w16: u16, h: u16, px: &[u8]) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend(0x10u32.to_le_bytes());
        d.extend((bpp_flag | if clut.is_some() { 8 } else { 0 }).to_le_bytes());
        if let Some(c) = clut {
            d.extend((12 + c.len() as u32 * 2).to_le_bytes());
            d.extend([0u16, 0, c.len() as u16, 1].iter().flat_map(|v| v.to_le_bytes()));
            d.extend(c.iter().flat_map(|v| v.to_le_bytes()));
        }
        d.extend((12 + px.len() as u32).to_le_bytes());
        d.extend([0u16, 0, w16, h].iter().flat_map(|v| v.to_le_bytes()));
        d.extend_from_slice(px);
        d
    }

    #[test]
    fn decodes_4bpp() {
        let mut clut = [0u16; 16];
        clut[1] = 0x001F; // red
        clut[2] = 0x7C00; // blue
        // 1 VRAM unit wide = 4 pixels: indices 1,2,0,1
        let tim = Tim::parse(&build(0, Some(&clut), 1, 1, &[0x21, 0x10])).unwrap();
        assert_eq!((tim.width, tim.height), (4, 1));
        let rgba = tim.to_rgba(0);
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[4..8], &[0, 0, 255, 255]);
        assert_eq!(rgba[11], 0, "index 0 / colour 0 is transparent");
    }

    #[test]
    fn decodes_16bpp() {
        let tim = Tim::parse(&build(2, None, 1, 1, &0x03E0u16.to_le_bytes())).unwrap();
        assert_eq!(tim.to_rgba(0), vec![0, 255, 0, 255]);
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(Tim::parse(&[0; 16]).is_err());
    }
}
