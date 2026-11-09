//! Render the full 320×240 solo screen to PNG after scripted play (no browser needed):
//! cargo run -p stackers-web --example screen -- <track01.bin> <out.png> [frame] [record] [idle]
//! `idle` plays no input in solo mode, so the stack tops out and the game-over sequence runs.
//! `pause` plays solo and presses Start 20 frames before the end; `confirm` then also picks Quit.
//! `chain` plays Chain Reaction (mode 7); `chainzoom` also holds L2 (zoom out) from frame 300;
//! `chaindrop` presses □ on every third pair. `trial` plays Time Trial (mode 11) on the course
//! given as `record` (0–4); `trialfill` also fills player 2 at frame 400 (a record);
//! `trialpause` presses Start 20 frames before the end; `trialselect` tops out and continues
//! into the in-game course list.
use stackers_core::pad;
use stackers_formats::{iso::Disc, tim::Tim};
use stackers_web::{Screen, Session, Sprites, Trial, SCREEN_STRIDE};
use std::collections::HashMap;

const W: usize = 320;
const H: usize = 240;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let raw = std::fs::read(&args[1]).expect("read track");
    let disc = Disc::open(&raw).unwrap();
    let read = |p: &str| disc.read_path(p).unwrap_or_else(|e| panic!("{p}: {e}"));
    let mut screen = Screen::new(&read("SLUS_003.15")).unwrap();
    screen.set_record(args.get(4).map_or(0, |s| s.parse().unwrap()));
    let mut sprites = Sprites::new(&read("DERODERO/DATA/BLT_ANM.OBJ")).unwrap();
    let pages: HashMap<i32, Tim> = screen
        .pages()
        .into_iter()
        .map(|p| (p, Tim::parse(&read(&screen.page_file(p).unwrap())).unwrap()))
        .collect();
    let last: u32 = args.get(3).map_or(900, |s| s.parse().unwrap());

    let mode = args.get(5).map_or("", String::as_str);
    let idle = mode == "idle";
    let pause = matches!(mode, "pause" | "confirm");
    let chain = mode.starts_with("chain");
    if mode.starts_with("trial") {
        let course: u32 = args.get(4).map_or(0, |s| s.parse().unwrap());
        let mut trial = Trial::new(7, &read("DERODERO/DATA/FIL2_DAT.OBJ"), course, vec![0; 5]).unwrap();
        trial.set_sd_chr(&read("DERODERO/DATA/SD_CHR.OBJ")).unwrap();
        let mut list = Vec::new();
        let mut over_at = None;
        for frame in 0..=last {
            let p = trial.piece();
            let target = 16 * (1 + (p[4] * 5 % 6));
            let held = match mode {
                "trialpause" if last - frame == 20 => pad::START,
                "trialselect" if over_at.is_some_and(|f| frame == f + 320) => pad::START,
                "trialselect" if over_at.is_some_and(|f| frame == f + 420) => pad::DOWN,
                "trialselect" => 0,
                _ if trial.sub() == 3 || p[0] == 0 => 0,
                _ if p[1] != target => if frame % 2 == 0 { if p[1] < target { pad::RIGHT } else { pad::LEFT } } else { 0 },
                _ => pad::DOWN,
            };
            if mode == "trialfill" && frame == 400 {
                trial.debug_fill_target();
            }
            trial.step(held);
            if over_at.is_none() && trial.finish() != 0 {
                over_at = Some(frame);
                eprintln!("finish {} at frame {frame}, clock {:06X}", trial.finish(), trial.clock());
            }
            list = screen.trial(&mut sprites, &mut trial);
        }
        eprintln!("sub {} course {} target {} clock {:06X}", trial.sub(), trial.course(), trial.target(), trial.clock());
        let bg = Tim::parse(&read(&screen.trial_background())).unwrap().to_rgba(0);
        let fb: Vec<u8> = bg.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        return write_png(&args[2], fb, &list, &pages);
    }
    let mut session = if chain {
        Session::chain_reaction(7, 3)
    } else if idle || pause {
        Session::solo(7)
    } else {
        Session::new(7, 0)
    };
    if chain {
        screen.set_title_bg(&read(&screen.title_bg_file())).unwrap();
    }
    session.set_sd_chr(&read("DERODERO/DATA/SD_CHR.OBJ")).unwrap();
    session.set_record(args.get(4).map_or(0, |s| s.parse().unwrap()));
    let mut list = Vec::new();
    for frame in 0..=last {
        let pieces = session.stats()[6];
        let target = 16 * (1 + (pieces * 7 % 6) as i32);
        let p = session.piece();
        let script = match last - frame {
            20 if pause => Some(pad::START),
            10 if mode == "confirm" => Some(pad::DOWN),
            4 if mode == "confirm" => Some(pad::CROSS),
            _ if session.paused() => Some(0),
            _ if mode == "chaindrop" && p[0] != 0 && pieces % 3 == 2 => Some(pad::SQUARE),
            _ => None,
        };
        let held = if let Some(b) = script {
            b
        } else if idle || p[0] == 0 {
            0
        } else if p[1] != target {
            if frame % 2 == 0 { if p[1] < target { pad::RIGHT } else { pad::LEFT } } else { 0 }
        } else {
            pad::DOWN
        };
        let held = if mode == "chainzoom" && frame >= 300 { held | pad::L2 } else { held };
        let was_over = session.stats()[8] != 0;
        session.step(held);
        if !was_over && session.stats()[8] != 0 {
            eprintln!("game over at frame {frame}");
        }
        list = if chain {
            let mut l = screen.chain_background();
            l.extend(screen.chain(&mut sprites, &session));
            l
        } else {
            screen.solo(&mut sprites, &session)
        };
    }
    if chain {
        eprintln!("chain/camera/zoom/state {:?}", session.chain());
    }

    let fb: Vec<u8> = if chain {
        [0, 0, 0, 255].repeat(W * H)
    } else {
        let bg = Tim::parse(&read(&screen.background())).unwrap().to_rgba(0);
        bg.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect()
    };
    write_png(&args[2], fb, &list, &pages);
}

fn write_png(path: &str, mut fb: Vec<u8>, list: &[i32], pages: &HashMap<i32, Tim>) {
    let mut decoded: HashMap<(i32, i32, bool), Vec<u8>> = HashMap::new();
    for s in list.chunks(SCREEN_STRIDE) {
        let [page, clut, u, v, w, h, x, y, dw, dh, flip, mode] = s.try_into().unwrap();
        let semi = mode & 0x100 != 0;
        let bright = mode & 0xFF;
        let tim = &pages[&page];
        let (u0, v0) = tim.page_origin();
        let (u, v) = (u - u0 as i32, v - v0 as i32);
        let rgba = decoded.entry((page, clut, semi)).or_insert_with(|| tim.to_rgba_semi(clut as usize, semi));
        for oy in 0..dh {
            for ox in 0..dw {
                let tx = ox * w / dw;
                let tx = if flip != 0 { u + w - 1 - tx } else { u + tx };
                let ty = v + oy * h / dh;
                // Some records reach past their TIM (JIYU55 is 176 rows; the original shows
                // whatever VRAM is there): leave those texels out.
                if tx < 0 || ty < 0 || tx >= tim.width as i32 || ty >= tim.height as i32 {
                    continue;
                }
                let p = &rgba[((ty * tim.width as i32 + tx) * 4) as usize..][..4];
                let (sx, sy) = (x + ox, y + oy);
                if p[3] == 0 || sx < 0 || sy < 0 || sx >= W as i32 || sy >= H as i32 {
                    continue;
                }
                let d = &mut fb[(sy as usize * W + sx as usize) * 4..][..4];
                for c in 0..3 {
                    let src = (i32::from(p[c]) * bright / 128).min(255);
                    let a = i32::from(p[3]);
                    d[c] = ((src * a + i32::from(d[c]) * (255 - a)) / 255) as u8;
                }
            }
        }
    }
    let mut enc = png::Encoder::new(std::fs::File::create(path).unwrap(), W as u32, H as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.write_header().unwrap().write_image_data(&fb).unwrap();
}
