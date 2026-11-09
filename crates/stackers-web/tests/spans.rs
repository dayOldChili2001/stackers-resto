//! The core's arm-span timings against the user's own `BLT_ANM.OBJ`. Skipped without a disc.

use stackers_core::tables::SPAN_SCRIPTS;
use stackers_formats::anm::{AnimBank, ScriptPlayer};
use stackers_formats::iso::Disc;

const TRACK01: &str = "../../rom/Tecmo Stackers (USA)/Tecmo Stackers (USA) (Track 01).bin";

#[test]
fn span_timings_match_the_disc() {
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let bank = AnimBank::parse(&disc.read_path("DERODERO/DATA/BLT_ANM.OBJ").unwrap()).unwrap();
    for (side, table) in SPAN_SCRIPTS.iter().enumerate() {
        for (i, timing) in table.iter().enumerate() {
            let script = bank.span_script(side, i).unwrap();
            let mut p = ScriptPlayer::span(script);
            let mut events = Vec::new();
            let mut done = None;
            for step in 1..=200u16 {
                p.step(&bank);
                if let Some(n) = p.event {
                    events.push((step as u8, n as u8));
                }
                if p.done {
                    done = Some(step as u8);
                    break;
                }
            }
            assert_eq!(done, Some(timing.done), "side {side} script {i}");
            assert_eq!(events, timing.events, "side {side} script {i}");
        }
    }
}
