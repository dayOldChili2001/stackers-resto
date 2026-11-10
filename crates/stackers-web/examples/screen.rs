//! Render the full 320×240 solo screen to PNG after scripted play (no browser needed):
//! cargo run -p stackers-web --example screen -- <track01.bin> <out.png> [frame] [record] [idle]
//! `idle` plays no input in solo mode, so the stack tops out and the game-over sequence runs.
//! `pause` plays solo and presses Start 20 frames before the end; `confirm` then also picks Quit.
//! `chain` plays Chain Reaction (mode 7); `chainzoom` also holds L2 (zoom out) from frame 300;
//! `chaindrop` presses □ on every third pair. `trial` plays Time Trial (mode 11) on the course
//! given as `record` (0–4); `trialfill` also fills player 2 at frame 400 (a record);
//! `trialpause` presses Start 20 frames before the end; `trialselect` tops out and continues
//! into the in-game course list. `match` plays an Arcade set match (mode 14) with two simple
//! bots, difficulty from `record` (1–5); `matchbomb` also has player 1 press □ at frame 250;
//! `matchchain` starts player 1's chain-2 cut-in and player 2's chain-5 one at frame 250;
//! `trialclear` starts player 1's all-clear sprite (mode 11's entry) at frame 400.
//! `matchclear` starts both players' all-clear sprites at frame 250 (entry 0, as modes 13–15).
//! `matchspin` starts player 1's special-column spin at frame 250 and gives player 2 incoming
//! garbage (1, 4, 7 and 30 blobs in columns 1, 2, 3 and 6). `matchrepick` jumps straight to
//! the re-pick lists: player 1 picks Hard with 2 bombs and confirms at frame 40, player 2 picks
//! 1 bomb and confirms at frame 80 (so the lists leave from frame 107). `cpu` plays vs CPU
//! (mode 13) on course `record` (0–2) against the AI, player 1 dropping in place; `cpuwin`
//! also tops out the CPU at frame 300 (the stage-won results show from about frame 482).
//! `cpustage` starts on stage `record` (0–13) of the Hard course instead. `demo` runs the
//! attract demo (mode 2) with no input; its first panel opens around frame 300.
use stackers_core::pad;
use stackers_formats::{iso::Disc, tim::Tim};
use stackers_web::{CpuMatch, Demo, Match, Screen, Session, Sprites, Trial, SCREEN_STRIDE};
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
    if mode == "ending" {
        // Mode 16 at frame `out` after course `arg % 10` was cleared; arg ≥ 10: with a continue
        // (last stage 13).
        let arg: u8 = args.get(4).map_or(0, |s| s.parse().unwrap());
        let course = arg % 10;
        let end = if course == 0 { 3 } else if arg >= 10 { 13 } else { 14 };
        let mut e = stackers_web::Ending::new(course, end);
        e.set_sd_chr(&read("DERODERO/DATA/SD_CHR.OBJ")).unwrap();
        let mut list = Vec::new();
        for _ in 0..=last {
            e.step();
            list = screen.ending(&e);
        }
        eprintln!("state {} staff page {} done {}", e.state(), e.staff_page(), e.done());
        let fb: Vec<u8> = [0, 0, 0, 255].repeat(W * H);
        return write_png(&args[2], fb, &list, &pages);
    }
    if mode.starts_with("match") {
        let d: u8 = args.get(4).map_or(2, |s| s.parse().unwrap());
        let mut m = Match::new(7, d, d, 1, 1, 3);
        m.set_sd_chr(&read("DERODERO/DATA/SD_CHR.OBJ")).unwrap();
        let mut list = Vec::new();
        if mode == "matchrepick" {
            let core = m.core_mut();
            core.sub = stackers_core::set_match::Sub::Repick;
            core.phase = stackers_core::set_match::repick::INIT;
            for frame in 0..=last {
                let p1 = match frame { 30 => pad::DOWN, 32 | 34 => pad::RIGHT, 40 => pad::CROSS, _ => 0 };
                let p2 = match frame { 30 => pad::LEFT, 33 => pad::RIGHT, 80 => pad::CROSS, _ => 0 };
                // Edges only: release between presses.
                m.step(p1, p2);
                list = screen.set_match(&mut sprites, &mut m);
            }
        }
        for frame in 0..=last {
            if mode == "matchrepick" {
                break;
            }
            // Player 1 drops in place; player 2 drifts right, then drops.
            let p1 = if mode == "matchbomb" && frame == 250 { pad::SQUARE } else if frame % 3 == 0 { pad::DOWN } else { 0 };
            let p2 = match frame % 40 { 0 => pad::RIGHT, 20 => pad::LEFT, f if f % 3 == 0 => pad::DOWN, _ => 0 };
            m.step(p1, p2);
            if mode == "matchchain" && frame == 250 {
                let games = &mut m.core_mut().versus.games;
                games[0].cut_in = Some(stackers_core::CutIn { entry: 2, age: 0 });
                games[1].cut_in = Some(stackers_core::CutIn { entry: 5, age: 0 });
            }
            if mode == "matchclear" && frame == 250 {
                for g in &mut m.core_mut().versus.games {
                    g.all_clear = Some(stackers_core::AllClear { entry: 0, steps: 0 });
                }
            }
            if mode == "matchspin" && frame == 250 {
                let core = m.core_mut();
                core.roulette[0].start(1);
                let g = &mut core.versus.games[1].garbage;
                g.incoming[1..7].copy_from_slice(&[1, 4, 7, 0, 0, 30]);
            }
            list = screen.set_match(&mut sprites, &mut m);
        }
        eprintln!("sub {} hud {:?}", m.sub(), m.hud());
        let bg = Tim::parse(&read(&screen.match_background(&m))).unwrap().to_rgba(0);
        let fb: Vec<u8> = bg.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        return write_png(&args[2], fb, &list, &pages);
    }
    if mode.starts_with("challenge") {
        // vs CPU (course 1) with the human on pad `arg`; at frame 60 the other pad presses
        // Start. `challenge`: mode 15 at frame `out`. `challengeback`: the human loses two sets,
        // nobody continues, and the winner's course is drawn at `out` frames after the return.
        let me: usize = args.get(4).map_or(0, |s| s.parse().unwrap());
        let sd = read("DERODERO/DATA/SD_CHR.OBJ");
        let [(c, a, b)] = [stackers_core::front::arcade::VS_COURSES[1]];
        let mut cpu = CpuMatch::new(7, me, c, a, b);
        cpu.set_sd_chr(&sd).unwrap();
        let start = |f: u32, p: usize| if f == 60 && p != me { pad::START } else { 0 };
        let mut frame = 0;
        while cpu.outcome() != 4 {
            cpu.step(start(frame, 0), start(frame, 1));
            frame += 1;
            assert!(frame < 2000);
        }
        let mut m = cpu.challenge(7).unwrap();
        m.set_sd_chr(&sd).unwrap();
        let mut list = Vec::new();
        if mode == "challenge" {
            for frame in 0..=last {
                let p = if frame % 3 == 0 { pad::DOWN } else { 0 };
                m.step(p, p);
                list = screen.set_match(&mut sprites, &mut m);
            }
            eprintln!("sub {} hud {:?} menu pad {}", m.sub(), m.hud(), m.menu_pad());
            let bg = Tim::parse(&read(&screen.match_background(&m))).unwrap().to_rgba(0);
            let fb: Vec<u8> = bg.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
            return write_png(&args[2], fb, &list, &pages);
        }
        let mut frame = 0;
        while m.outcome() == 0 {
            // The human tops out in each set; the countdown runs out.
            let core = m.core_mut();
            if core.sub == stackers_core::set_match::Sub::Play && frame % 200 == 199 {
                core.versus.games[me].player.state = stackers_core::game::ps::GAME_OVER;
            }
            m.step(0, 0);
            screen.set_match(&mut sprites, &mut m);
            frame += 1;
            assert!(frame < 10_000);
        }
        let mut cpu = m.course(7).expect("back to the course");
        cpu.set_sd_chr(&sd).unwrap();
        for frame in 0..=last {
            let p = if frame % 3 == 0 { pad::DOWN } else { 0 };
            cpu.step(p, p);
            list = screen.vs_cpu(&mut sprites, &mut cpu);
        }
        eprintln!("sub {} hud {:?} menu pad {}", cpu.sub(), cpu.hud(), cpu.menu_pad());
        let bg = Tim::parse(&read(&screen.vs_cpu_background(&cpu))).unwrap().to_rgba(0);
        let fb: Vec<u8> = bg.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        return write_png(&args[2], fb, &list, &pages);
    }
    if mode == "demo" {
        let mut d = Demo::new(7);
        d.set_sd_chr(&read("DERODERO/DATA/SD_CHR.OBJ")).unwrap();
        let mut list = Vec::new();
        for _ in 0..=last {
            d.step(0, 0);
            list = screen.demo(&mut sprites, &mut d);
        }
        eprintln!("sub {} panel {} done {}", d.sub(), d.panel(), d.done());
        let bg = Tim::parse(&read(&screen.demo_background())).unwrap().to_rgba(0);
        let fb: Vec<u8> = bg.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        return write_png(&args[2], fb, &list, &pages);
    }
    if mode.starts_with("cpu") {
        let r: u8 = args.get(4).map_or(0, |s| s.parse().unwrap());
        let mut m = if mode == "cpustage" {
            CpuMatch::new(7, 0, 2, r, 14)
        } else {
            let [(c, a, b)] = [stackers_core::front::arcade::VS_COURSES[usize::from(r.min(2))]];
            CpuMatch::new(7, 0, c, a, b)
        };
        m.set_sd_chr(&read("DERODERO/DATA/SD_CHR.OBJ")).unwrap();
        let mut list = Vec::new();
        for frame in 0..=last {
            let p1 = if frame % 3 == 0 { pad::DOWN } else { 0 };
            m.step(p1, 0);
            if mode == "cpuwin" && frame == 300 {
                m.core_mut().versus.games[1].player.state = stackers_core::game::ps::GAME_OVER;
            }
            list = screen.vs_cpu(&mut sprites, &mut m);
        }
        eprintln!("sub {} hud {:?}", m.sub(), m.hud());
        let bg = Tim::parse(&read(&screen.vs_cpu_background(&m))).unwrap().to_rgba(0);
        let fb: Vec<u8> = bg.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        return write_png(&args[2], fb, &list, &pages);
    }
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
            if mode == "trialclear" && frame == 400 {
                trial.debug_all_clear();
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
        let Some(tim) = pages.get(&page) else { eprintln!("no page {page}"); continue };
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
