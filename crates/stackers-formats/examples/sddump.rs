//! Trace SD_CHR scripts: cargo run --example sddump -- <track01.bin> <id> [frames]
//! Prints each object's position, scale and frame (page, u, v, w, h) whenever they change.
use stackers_formats::{iso::Disc, sdchr::{SdBank, SdStage}};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let raw = std::fs::read(&args[1]).expect("read track");
    let disc = Disc::open(&raw).unwrap();
    let bank = SdBank::parse(&disc.read_path("DERODERO/DATA/SD_CHR.OBJ").unwrap()).unwrap();
    let id = u8::from_str_radix(args[2].trim_start_matches("0x"), 16).unwrap();
    let frames: u32 = args.get(3).map_or(300, |s| s.parse().unwrap());
    let mut s = SdStage::default();
    s.start(&bank, id);
    let mut last = vec![String::new(); 25];
    let mut vnao = [0; 2];
    for t in 0..frames {
        let se = s.step(&bank);
        if s.vnao != vnao {
            vnao = s.vnao;
            println!("{t:4} vnao pages 26/27 = {vnao:?}");
        }
        if !se.is_empty() {
            println!("{t:4} se {se:?}");
        }
        for (i, o) in s.objects.iter().enumerate() {
            let line = match (o.visible, o.frame) {
                (true, Some(f)) => format!(
                    "L{} ({},{}) s{:?} p{} c{} uv({},{}) {}x{} d({},{}) f{}",
                    o.layer, o.x, o.y, o.scale, f.page, f.clut, f.u, f.v, f.w, f.h, f.dx, f.dy, o.flip
                ),
                _ => "-".into(),
            };
            if line != last[i] {
                println!("{t:4} #{i} {line}");
                last[i] = line;
            }
        }
    }
}
