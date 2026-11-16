//! Pixel-art upscaling for the page's "Smooth" look: MMPX (Morgan McGuire and Mara Gagiu,
//! "MMPX Style-Preserving Pixel Art Magnification", JCGT 2021, MIT licence). It doubles an
//! image, rounding diagonals and curves while keeping one-pixel lines, text strokes and the
//! original palette (every output pixel is a copy of an input pixel). 4× is MMPX applied twice.
//!
//! Pixels are RGBA bytes. Fully transparent pixels are treated as one colour (whatever their
//! RGB), and edges clamp, so a sprite cut out of a sheet never picks up its neighbours.

fn pack(rgba: &[u8], w: usize, h: usize) -> Vec<u32> {
    assert_eq!(rgba.len(), w * h * 4, "rgba is not {w}×{h}");
    rgba.chunks_exact(4)
        .map(|p| if p[3] == 0 { 0 } else { u32::from_le_bytes([p[0], p[1], p[2], p[3]]) })
        .collect()
}

fn unpack(px: &[u32]) -> Vec<u8> {
    px.iter().flat_map(|p| p.to_le_bytes()).collect()
}

/// The reference's luma: R+G+B, weighted so more transparent pixels count as brighter.
fn luma(c: u32) -> u32 {
    let [r, g, b, a] = c.to_le_bytes();
    (r as u32 + g as u32 + b as u32 + 1) * (256 - a as u32)
}

fn all_eq2(b: u32, a0: u32, a1: u32) -> bool {
    b == a0 && b == a1
}
fn all_eq3(b: u32, a0: u32, a1: u32, a2: u32) -> bool {
    b == a0 && b == a1 && b == a2
}
fn all_eq4(b: u32, a0: u32, a1: u32, a2: u32, a3: u32) -> bool {
    b == a0 && b == a1 && b == a2 && b == a3
}
fn any_eq3(b: u32, a0: u32, a1: u32, a2: u32) -> bool {
    b == a0 || b == a1 || b == a2
}
fn none_eq2(b: u32, a0: u32, a1: u32) -> bool {
    b != a0 && b != a1
}
fn none_eq4(b: u32, a0: u32, a1: u32, a2: u32, a3: u32) -> bool {
    b != a0 && b != a1 && b != a2 && b != a3
}

fn mmpx_px(src: &[u32], w: usize, h: usize) -> Vec<u32> {
    let at = |x: isize, y: isize| -> u32 {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        src[y * w + x]
    };
    let ow = w * 2;
    let mut out = vec![0u32; ow * h * 2];
    for sy in 0..h as isize {
        for sx in 0..w as isize {
            let (a, b, c) = (at(sx - 1, sy - 1), at(sx, sy - 1), at(sx + 1, sy - 1));
            let (d, e, f) = (at(sx - 1, sy), at(sx, sy), at(sx + 1, sy));
            let (g, hh, i) = (at(sx - 1, sy + 1), at(sx, sy + 1), at(sx + 1, sy + 1));
            // J K / L M: the four output pixels, top-left to bottom-right.
            let (mut j, mut k, mut l, mut m) = (e, e, e, e);
            if a != e || b != e || c != e || d != e || f != e || g != e || hh != e || i != e {
                let p = at(sx, sy - 2);
                let s = at(sx, sy + 2);
                let q = at(sx - 2, sy);
                let r = at(sx + 2, sy);
                let (bl, dl, el, fl, hl) = (luma(b), luma(d), luma(e), luma(f), luma(hh));

                // 1:1 slopes
                if (d == b && d != hh && d != f)
                    && (el >= dl || e == a)
                    && any_eq3(e, a, c, g)
                    && (el < dl || a != d || e != p || e != q)
                {
                    j = d;
                }
                if (b == f && b != d && b != hh)
                    && (el >= bl || e == c)
                    && any_eq3(e, a, c, i)
                    && (el < bl || c != b || e != p || e != r)
                {
                    k = b;
                }
                if (hh == d && hh != f && hh != b)
                    && (el >= hl || e == g)
                    && any_eq3(e, a, g, i)
                    && (el < hl || g != hh || e != s || e != q)
                {
                    l = hh;
                }
                if (f == hh && f != b && f != d)
                    && (el >= fl || e == i)
                    && any_eq3(e, c, g, i)
                    && (el < fl || i != hh || e != r || e != s)
                {
                    m = f;
                }

                // Intersections
                if (e != f && all_eq4(e, c, i, d, q) && all_eq2(f, b, hh)) && f != at(sx + 3, sy) {
                    k = f;
                    m = f;
                }
                if (e != d && all_eq4(e, a, g, f, r) && all_eq2(d, b, hh)) && d != at(sx - 3, sy) {
                    j = d;
                    l = d;
                }
                if (e != hh && all_eq4(e, g, i, b, p) && all_eq2(hh, d, f)) && hh != at(sx, sy + 3) {
                    l = hh;
                    m = hh;
                }
                if (e != b && all_eq4(e, a, c, hh, s) && all_eq2(b, d, f)) && b != at(sx, sy - 3) {
                    j = b;
                    k = b;
                }

                // Triangle tips
                if bl < el && all_eq4(e, g, hh, i, s) && none_eq4(e, a, d, c, f) {
                    j = b;
                    k = b;
                }
                if hl < el && all_eq4(e, a, b, c, p) && none_eq4(e, d, g, i, f) {
                    l = hh;
                    m = hh;
                }
                if fl < el && all_eq4(e, a, d, g, q) && none_eq4(e, b, c, i, hh) {
                    k = f;
                    m = f;
                }
                if dl < el && all_eq4(e, c, f, i, r) && none_eq4(e, b, a, g, hh) {
                    j = d;
                    l = d;
                }

                // 2:1 slopes
                if hh != b {
                    if hh != a && hh != e && hh != c {
                        if all_eq3(hh, g, f, r) && none_eq2(hh, d, at(sx + 2, sy - 1)) {
                            l = m;
                        }
                        if all_eq3(hh, i, d, q) && none_eq2(hh, f, at(sx - 2, sy - 1)) {
                            m = l;
                        }
                    }
                    if b != i && b != g && b != e {
                        if all_eq3(b, a, f, r) && none_eq2(b, d, at(sx + 2, sy + 1)) {
                            j = k;
                        }
                        if all_eq3(b, c, d, q) && none_eq2(b, f, at(sx - 2, sy + 1)) {
                            k = j;
                        }
                    }
                }
                if f != d {
                    if d != i && d != e && d != c {
                        if all_eq3(d, a, hh, s) && none_eq2(d, b, at(sx + 1, sy + 2)) {
                            j = l;
                        }
                        if all_eq3(d, g, b, p) && none_eq2(d, hh, at(sx + 1, sy - 2)) {
                            l = j;
                        }
                    }
                    if f != e && f != a && f != g {
                        if all_eq3(f, c, hh, s) && none_eq2(f, b, at(sx - 1, sy + 2)) {
                            k = m;
                        }
                        if all_eq3(f, i, b, p) && none_eq2(f, hh, at(sx - 1, sy - 2)) {
                            m = k;
                        }
                    }
                }
            }
            let (ox, oy) = (sx as usize * 2, sy as usize * 2);
            out[oy * ow + ox] = j;
            out[oy * ow + ox + 1] = k;
            out[(oy + 1) * ow + ox] = l;
            out[(oy + 1) * ow + ox + 1] = m;
        }
    }
    out
}

/// MMPX 2×: `rgba` is `w`×`h`; the result is `2w`×`2h`.
pub fn mmpx2(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 {
        return Vec::new();
    }
    unpack(&mmpx_px(&pack(rgba, w, h), w, h))
}

/// MMPX twice: `rgba` is `w`×`h`; the result is `4w`×`4h`.
pub fn upscale4(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let once = mmpx_px(&pack(rgba, w, h), w, h);
    unpack(&mmpx_px(&once, w * 2, h * 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    fn image(rows: &[&str]) -> (Vec<u8>, usize, usize) {
        let (w, h) = (rows[0].len(), rows.len());
        let px = rows.iter().flat_map(|r| r.bytes().flat_map(|c| if c == b'#' { RED } else { CLEAR })).collect();
        (px, w, h)
    }

    fn get(px: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * w + x) * 4;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    #[test]
    fn flat_colour_stays_flat() {
        let px: Vec<u8> = [12, 34, 56, 255].repeat(5 * 3);
        let out = upscale4(&px, 5, 3);
        assert_eq!(out.len(), 20 * 12 * 4);
        assert!(out.chunks_exact(4).all(|p| p == [12, 34, 56, 255]));
    }

    #[test]
    fn transparent_stays_transparent() {
        // Transparent pixels with junk RGB come out as plain transparent.
        let px: Vec<u8> = [9, 9, 9, 0].repeat(4 * 4);
        assert!(upscale4(&px, 4, 4).chunks_exact(4).all(|p| p == CLEAR));
    }

    #[test]
    fn one_pixel_line_stays_connected() {
        let (px, w, h) = image(&["......", "######", "......"]);
        let out = mmpx2(&px, w, h);
        // The line is two output rows thick and unbroken across the whole width.
        for x in 0..w * 2 {
            assert_eq!(get(&out, w * 2, x, 2), RED, "gap at x={x}");
            assert_eq!(get(&out, w * 2, x, 3), RED, "gap at x={x}");
            assert_eq!(get(&out, w * 2, x, 1), CLEAR);
            assert_eq!(get(&out, w * 2, x, 4), CLEAR);
        }
    }

    #[test]
    fn staircase_gets_smoothed() {
        // A filled lower-left triangle: nearest-neighbour would leave 2×2 steps; MMPX fills
        // half of each step on the diagonal.
        let (px, w, h) = image(&["#.....", "##....", "###...", "####..", "#####.", "######"]);
        let out = mmpx2(&px, w, h);
        let nearest_red = (0..h).map(|y| (y + 1) * 4).sum::<usize>();
        let red = out.chunks_exact(4).filter(|p| *p == RED).count();
        assert!(red > nearest_red, "{red} red pixels, nearest has {nearest_red}");
    }

    #[test]
    fn output_is_from_the_input_palette() {
        let (px, w, h) = image(&["#.#.", ".##.", "#..#", ".#.#"]);
        assert!(upscale4(&px, w, h).chunks_exact(4).all(|p| p == RED || p == CLEAR));
    }
}
