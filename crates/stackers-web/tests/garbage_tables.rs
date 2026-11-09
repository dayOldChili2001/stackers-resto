//! The core's garbage tables against the user's own executable. Skipped without a disc.

use stackers_core::tables::{ALL_CLEAR_BONUS, GARBAGE_DIVISOR, GARBAGE_SPREAD};
use stackers_formats::iso::Disc;

const TRACK01: &str = "../../rom/Tecmo Stackers (USA)/Tecmo Stackers (USA) (Track 01).bin";

#[test]
fn garbage_tables_match_the_exe() {
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let exe = disc.read_path("SLUS_003.15").unwrap();
    let load = u32::from_le_bytes(exe[0x18..0x1C].try_into().unwrap());
    let at = |addr: u32| (addr - load) as usize + 0x800;
    let i16_at = |a: u32| i16::from_le_bytes(exe[at(a)..at(a) + 2].try_into().unwrap());
    let u32_at = |a: u32| u32::from_le_bytes(exe[at(a)..at(a) + 4].try_into().unwrap());

    for (i, &d) in GARBAGE_DIVISOR.iter().enumerate() {
        assert_eq!(d, i16_at(0x8005_7680 + 2 * i as u32), "divisor {i}");
    }
    for (i, &b) in ALL_CLEAR_BONUS.iter().enumerate() {
        assert_eq!(b, u32_at(0x8005_76C8 + 4 * i as u32), "all clear {i}");
    }
    for (n, rec) in GARBAGE_SPREAD.iter().enumerate() {
        let o = at(0x8005_7152 + 22 * n as u32);
        assert_eq!(rec.count, exe[o], "spread {n} count");
        assert_eq!(rec.count4, exe[o + 1], "spread {n} count4");
        assert_eq!(rec.masks[..], exe[o + 2..o + 22], "spread {n} masks");
    }
}
