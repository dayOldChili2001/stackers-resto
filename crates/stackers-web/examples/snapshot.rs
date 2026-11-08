//! Render the field with real sprites to PNG after scripted play (no browser needed):
//! cargo run -p stackers-web --example snapshot -- <track01.bin> <out.png> [frames...]
use stackers_core::pad;
use stackers_formats::{iso::Disc, tim::Tim};
use stackers_web::{Session, Sprites};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let raw = std::fs::read(&args[1]).expect("read track");
    let disc = Disc::open(&raw).unwrap();
    let mut sprites = Sprites::new(&disc.read_path("DERODERO/DATA/BLT_ANM.OBJ").unwrap()).unwrap();
    let sheets: Vec<Tim> = (0..10)
        .map(|i| Tim::parse(&disc.read_path(&format!("DERODERO/GRAPH/DERO/DERO{i}.TIM")).unwrap()).unwrap())
        .collect();
    let shots: Vec<u32> = args[3..].iter().map(|s| s.parse().unwrap()).collect();
    let last = *shots.iter().max().unwrap_or(&2000);
    let (w, h) = (128usize, 304usize);
    let mut strip = vec![0u8; w * shots.len().max(1) * h * 4];
    let total_w = w * shots.len().max(1);
    let mut session = Session::new(7, 0);
    for frame in 0..=last {
        // Aim each piece at a column chosen from the piece count, then drop it.
        let pieces = session.stats()[6];
        let target = 16 * (1 + (pieces * 7 % 6) as i32);
        let p = session.piece();
        let held = if p[0] == 0 {
            0
        } else if p[1] < target {
            if frame % 2 == 0 { pad::RIGHT } else { 0 }
        } else if p[1] > target {
            if frame % 2 == 0 { pad::LEFT } else { 0 }
        } else if pieces % 3 == 0 && frame % 40 == 0 {
            pad::CROSS
        } else {
            pad::DOWN
        };
        session.step(held);
        let list = sprites.field(&session);
        if let Some(k) = shots.iter().position(|&s| s == frame) {
            let ox = k * w;
            for y in 0..h {
                for x in 0..w {
                    let wall = x < 16 || x >= 112 || y >= 288;
                    let c = if wall { [60, 60, 70, 255] } else { [30, 30, 38, 255] };
                    strip[(y * total_w + ox + x) * 4..][..4].copy_from_slice(&c);
                }
            }
            for s in list.chunks(9) {
                let [page, clut, u, v, sw, sh, sx, sy, flip] = s.try_into().unwrap();
                let rgba = sheets[page as usize].to_rgba(clut as usize);
                let tw = sheets[page as usize].width as i32;
                for dy in 0..sh {
                    for dx in 0..sw {
                        let srcx = if flip != 0 { u + sw - 1 - dx } else { u + dx };
                        let p = &rgba[((v + dy) * tw + srcx) as usize * 4..][..4];
                        let (x, y) = (sx + dx, sy + dy);
                        if p[3] == 0 || x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                            continue;
                        }
                        strip[(y as usize * total_w + ox + x as usize) * 4..][..4].copy_from_slice(p);
                    }
                }
            }
        }
    }
    let mut enc = png::Encoder::new(std::fs::File::create(&args[2]).unwrap(), total_w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.write_header().unwrap().write_image_data(&strip).unwrap();
}
