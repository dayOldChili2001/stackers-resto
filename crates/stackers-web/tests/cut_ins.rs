//! The core's cut-in end times against the user's own SD_CHR.OBJ. Skipped without a disc.

use stackers_core::tables::CUT_IN_ENDS;
use stackers_formats::iso::Disc;
use stackers_formats::sdchr::SdBank;

const TRACK01: &str = "../../rom/Tecmo Stackers (USA)/Tecmo Stackers (USA) (Track 01).bin";

#[test]
fn cut_in_ends_match_the_scripts() {
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let bank = SdBank::parse(&disc.read_path("DERODERO/DATA/SD_CHR.OBJ").unwrap()).unwrap();
    for (entry, ends) in CUT_IN_ENDS.iter().enumerate() {
        for (object, &end) in ends.iter().enumerate() {
            let steps = bank.cut_in(object, entry as u8);
            assert!(!steps.is_empty(), "entry {entry} object {object}");
            let total: u32 = steps.iter().map(|s| s.0).sum();
            assert_eq!(u32::from(end), total, "entry {entry} object {object}");
        }
    }
    // Chain 2's text is the first RENSA2 cell; the bomb's burst is on RENSA1.
    let f = bank.cut_in_frame(0, 2, 0).unwrap();
    assert_eq!((f.page, f.u, f.v, f.w, f.h), (29, 0, 0, 40, 56));
    assert_eq!(bank.cut_in_frame(0, 2, 44), None);
    assert_eq!(bank.cut_in_frame(2, 15, 43).unwrap().page, 28);
}
