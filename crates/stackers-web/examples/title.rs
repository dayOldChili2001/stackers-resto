//! Render the title / menu to PNG after scripted input (no browser needed):
//! cargo run -p stackers-web --example title -- <track01.bin> <out.png> <frame> [button@frame,...]
//! Buttons: start, cross, triangle, up, down. A press lasts one frame. Frames count from power-on.
use stackers_core::pad;
use stackers_formats::{iso::Disc, tim::Tim};
use stackers_web::{FrontEnd, Screen, SCREEN_STRIDE};
use std::collections::HashMap;

const W: usize = 320;
const H: usize = 240;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let raw = std::fs::read(&args[1]).expect("read track");
    let disc = Disc::open(&raw).unwrap();
    let read = |p: &str| disc.read_path(p).unwrap_or_else(|e| panic!("{p}: {e}"));
    let mut screen = Screen::new(&read("SLUS_003.15")).unwrap();
    screen.set_title_bg(&read(&screen.title_bg_file())).unwrap();
    let pages: HashMap<i32, Tim> = screen
        .pages()
        .into_iter()
        .map(|p| (p, Tim::parse(&read(&screen.page_file(p).unwrap())).unwrap()))
        .collect();
    let last: u32 = args[3].parse().unwrap();
    let presses: HashMap<u32, u16> = args
        .get(4)
        .map(|s| {
            s.split(',')
                .map(|e| {
                    let (b, f) = e.split_once('@').unwrap();
                    let bit = match b {
                        "start" => pad::START,
                        "cross" => pad::CROSS,
                        "triangle" => pad::TRIANGLE,
                        "up" => pad::UP,
                        "down" => pad::DOWN,
                        _ => panic!("button {b}"),
                    };
                    (f.parse().unwrap(), bit)
                })
                .collect()
        })
        .unwrap_or_default();

    let mut fe = FrontEnd::new();
    fe.set_sd_chr(&read("DERODERO/DATA/SD_CHR.OBJ")).unwrap();
    let (mut list, mut spin) = (Vec::new(), Vec::new());
    for frame in 0..=last {
        let before = (fe.mode(), fe.music());
        fe.step(presses.get(&frame).copied().unwrap_or(0));
        if fe.mode() != before.0 || fe.music() != -1 || !fe.sounds().is_empty() {
            eprintln!("frame {frame}: mode {} music {} sounds {:?}", fe.mode(), fe.music(), fe.sounds());
        }
        if fe.launch_solo() {
            eprintln!("frame {frame}: solo starts");
            break;
        }
        list = screen.title(&fe);
        spin = screen.title_spin(&fe);
    }

    let mut fb = vec![0u8; W * H * 4];
    for p in fb.chunks_mut(4) {
        p[3] = 255;
    }
    let mut decoded: HashMap<(i32, i32), Vec<u8>> = HashMap::new();
    let mut texel = |page: i32, clut: i32, u: i32, v: i32| -> Option<[u8; 4]> {
        let tim = &pages[&page];
        let (u0, v0) = tim.page_origin();
        let (u, v) = (u - u0 as i32, v - v0 as i32);
        if u < 0 || v < 0 || u >= tim.width as i32 || v >= tim.height as i32 {
            return None;
        }
        let rgba = decoded.entry((page, clut)).or_insert_with(|| tim.to_rgba(clut as usize));
        let p = &rgba[((v * tim.width as i32 + u) * 4) as usize..][..4];
        (p[3] != 0).then(|| [p[0], p[1], p[2], p[3]])
    };
    let plot = |fb: &mut Vec<u8>, sx: i32, sy: i32, p: [u8; 4], bright: i32| {
        if sx < 0 || sy < 0 || sx >= W as i32 || sy >= H as i32 {
            return;
        }
        let d = &mut fb[(sy as usize * W + sx as usize) * 4..][..4];
        for c in 0..3 {
            d[c] = (i32::from(p[c]) * bright / 128).min(255) as u8;
        }
    };
    for s in list.chunks(SCREEN_STRIDE) {
        let [page, clut, u, v, w, h, x, y, dw, dh, flip, mode] = s.try_into().unwrap();
        for oy in 0..dh {
            for ox in 0..dw {
                let tx = ox * w / dw;
                let tx = if flip != 0 { u + w - 1 - tx } else { u + tx };
                if let Some(p) = texel(page, clut, tx, v + oy * h / dh) {
                    plot(&mut fb, x + ox, y + oy, p, mode & 0xFF);
                }
            }
        }
    }
    if let [page, clut, u, v, w, h, cx, cy, scale, rotate] = spin[..] {
        // Inverse-map every screen pixel through the rotation and scale.
        let k = f64::from(scale) / 4096.0;
        let a = f64::from(rotate) / 4096.0 * std::f64::consts::PI / 180.0;
        if k > 0.0 {
            for sy in 0..H as i32 {
                for sx in 0..W as i32 {
                    let (dx, dy) = (f64::from(sx - cx), f64::from(sy - cy));
                    let (rx, ry) = ((dx * a.cos() + dy * a.sin()) / k, (-dx * a.sin() + dy * a.cos()) / k);
                    let (tx, ty) = ((rx + f64::from(w) / 2.0).floor() as i32, (ry + f64::from(h) / 2.0).floor() as i32);
                    if tx >= 0 && ty >= 0 && tx < w && ty < h {
                        if let Some(p) = texel(page, clut, u + tx, v + ty) {
                            plot(&mut fb, sx, sy, p, 128);
                        }
                    }
                }
            }
        }
    }
    let mut enc = png::Encoder::new(std::fs::File::create(&args[2]).unwrap(), W as u32, H as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.write_header().unwrap().write_image_data(&fb).unwrap();
}
