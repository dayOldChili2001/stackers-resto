//! Dump TIMs from the user's disc to PNG: cargo run --example tim2png -- <track01.bin> <out_dir> [PATH_SUBSTR...]
use stackers_formats::{iso::Disc, tim::Tim};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let raw = std::fs::read(&args[1]).expect("read track");
    let disc = Disc::open(&raw).expect("iso");
    let out = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(out).unwrap();
    for e in disc.entries().iter().filter(|e| e.path.ends_with(".TIM")) {
        if args.len() > 3 && !args[3..].iter().any(|s| e.path.contains(s.as_str())) {
            continue;
        }
        let tim = Tim::parse(&disc.read(e).unwrap()).unwrap();
        let name = out.join(e.path.replace('/', "_").replace(".TIM", ".png"));
        let mut enc = png::Encoder::new(std::fs::File::create(&name).unwrap(), tim.width, tim.height);
        enc.set_color(png::ColorType::Rgba);
        enc.write_header().unwrap().write_image_data(&tim.to_rgba(0)).unwrap();
        println!("{} {}x{} {}bpp", name.display(), tim.width, tim.height, tim.bpp);
    }
}
