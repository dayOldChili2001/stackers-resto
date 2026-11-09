//! The two-player strip's core tables (`stackers_core::roulette`, `hud::strip`) against the
//! user's own disc. Skipped without one.

use stackers_core::roulette::{Op, INDICATOR_SCRIPTS, ROLL_OUT_FRAMES, WOBBLE};
use stackers_formats::anm::{AnimBank, HUD_DIGITS, HUD_INDICATOR};
use stackers_formats::hud::strip::COLUMN_X;
use stackers_formats::iso::Disc;

const TRACK01: &str = "../../rom/Tecmo Stackers (USA)/Tecmo Stackers (USA) (Track 01).bin";

#[test]
fn strip_tables_match_the_disc() {
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let exe = disc.read_path("SLUS_003.15").unwrap();
    let load = u32::from_le_bytes(exe[0x18..0x1C].try_into().unwrap());
    let at = |addr: u32| (addr - load) as usize + 0x800;
    let i16_at = |a: u32| i16::from_le_bytes(exe[at(a)..at(a) + 2].try_into().unwrap());

    // D_800557F4: the wobble, then the 0x80 end marker.
    for (i, &w) in WOBBLE.iter().enumerate() {
        assert_eq!(i16::from(w), i16_at(0x8005_57F4 + 2 * i as u32), "wobble {i}");
    }
    assert_eq!(i16_at(0x8005_57F4 + 2 * WOBBLE.len() as u32), 0x80);
    // D_800557C4: (x, y) per column, 0x18 bytes per player.
    for (p, xs) in COLUMN_X.iter().enumerate() {
        for (c, &x) in xs.iter().enumerate() {
            let a = 0x8005_57C4 + 0x18 * p as u32 + 4 * c as u32;
            assert_eq!((i32::from(i16_at(a)), i16_at(a + 2)), (x, 0), "column x {p} {c}");
        }
    }

    let bank = AnimBank::parse(&disc.read_path("DERODERO/DATA/BLT_ANM.OBJ").unwrap()).unwrap();
    let starts: Vec<u32> = (0..3).map(|i| bank.table_script(HUD_INDICATOR, i).unwrap()).collect();
    for (n, ops) in INDICATOR_SCRIPTS.iter().enumerate() {
        let (words, target) = bank.script_words(starts[n]).unwrap();
        let mut got = Vec::new();
        let mut w = words.iter();
        while let Some(&(a, b)) = w.next() {
            got.push(match (a, b) {
                (0xFFFF, 0) => Op::Jump(starts.iter().position(|&s| Some(s) == target).unwrap() as u8),
                (0xFFFF, 2) => Op::Clut(w.next().unwrap().0 as u8),
                (0xFFFF, 3) => Op::Step,
                (0xFFFF, 4) => Op::Wobble,
                (0xFFFF, op) => panic!("indicator script {n}: op {op}"),
                (frames, _) => Op::Show(frames as u8),
            });
        }
        assert_eq!(&got[..], *ops, "indicator script {n}");
    }

    // The digit scripts: 30 + n rolls n out in ROLL_OUT_FRAMES 1-frame pairs, then op 5.
    for n in 0..30u32 {
        let (words, target) = bank.script_words(bank.table_script(HUD_DIGITS, 30 + n).unwrap()).unwrap();
        assert_eq!(target, None);
        let pairs: Vec<_> = words.iter().take_while(|w| w.0 != 0xFFFF).collect();
        let want = if n == 0 { 0 } else { usize::from(ROLL_OUT_FRAMES) };
        assert_eq!(pairs.len(), want, "roll-out {n}");
        assert!(pairs.iter().all(|w| w.0 == 1));
        assert_eq!(words[pairs.len()], (0xFFFF, 5), "roll-out {n} ends with op 5");
    }
}
