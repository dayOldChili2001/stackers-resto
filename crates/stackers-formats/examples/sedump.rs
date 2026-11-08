//! List the sound-effect table: cargo run --example sedump -- <track01.bin>
//! One line per effect id: bank slot, bank name/kind, program, key.
use stackers_formats::{iso::Disc, sound::SoundTables};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let raw = std::fs::read(&args[1]).expect("read track");
    let disc = Disc::open(&raw).unwrap();
    let t = SoundTables::from_exe(&disc.read_path("SLUS_003.15").unwrap()).unwrap();
    for (i, b) in t.banks.iter().enumerate() {
        println!("bank {i:2} {:8} kind {}", b.name, b.kind);
    }
    for (i, e) in t.effects.iter().enumerate() {
        let b = &t.banks[usize::from(e.bank)];
        println!("{i:3} 0x{i:02X} bank {:2} {:8} prog {:3} key {:3}", e.bank, b.name, e.program, e.key);
    }
}
