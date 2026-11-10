//! The core's all-clear sprite length against the user's own BLT_ANM.OBJ. Skipped without a disc.

use stackers_core::tables::ALL_CLEAR_FRAMES;
use stackers_formats::anm::AnimBank;
use stackers_formats::iso::Disc;

const TRACK01: &str = "../../rom/Tecmo Stackers (USA)/Tecmo Stackers (USA) (Track 01).bin";

#[test]
fn all_clear_scripts_match_the_core() {
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let bank = AnimBank::parse(&disc.read_path("DERODERO/DATA/BLT_ANM.OBJ").unwrap()).unwrap();
    // Entry 0 on RULET (modes 13–15), entry 1 on OJAWAKU2 (mode 11), both page 21: 96×96.
    for (entry, (u, v)) in [(0, (0, 120)), (1, (112, 0))] {
        let script = bank.table_script(0x40, entry).unwrap();
        let (words, jump) = bank.script_words(script).unwrap();
        assert_eq!(jump, None, "entry {entry} ends with done");
        let total: u32 = words.iter().filter(|w| w.0 != 0xFFFF).map(|w| w.0).sum();
        assert_eq!(total, u32::from(ALL_CLEAR_FRAMES), "entry {entry}");
        let f = bank.frame(words[0].1).unwrap();
        assert_eq!((f.page, f.u, f.v, f.w, f.h), (21, u, v, 96, 96), "entry {entry}");
    }
}
