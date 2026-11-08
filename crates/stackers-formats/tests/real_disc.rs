//! Checks against the user's own disc. Skipped when the image is absent (e.g. in CI).

use stackers_formats::{iso::Disc, tim::Tim};

const TRACK01: &str = "../../rom/Tecmo Stackers (USA)/Tecmo Stackers (USA) (Track 01).bin";

#[test]
fn every_tim_on_disc_decodes() {
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).expect("ISO9660");
    assert!(disc.find("SLUS_003.15").is_some());
    let tims: Vec<_> = disc.entries().iter().filter(|e| e.path.ends_with(".TIM")).collect();
    assert!(tims.len() > 150, "found {} TIMs", tims.len());
    for e in tims {
        let tim = Tim::parse(&disc.read(e).unwrap()).unwrap_or_else(|err| panic!("{}: {err}", e.path));
        let rgba = tim.to_rgba(0);
        assert_eq!(rgba.len(), (tim.width * tim.height * 4) as usize, "{}", e.path);
    }
}

#[test]
fn blob_scripts_match_the_core_timings() {
    use stackers_formats::anm::{AnimBank, CellKey, ScriptPlayer};
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let bank = AnimBank::parse(&disc.read_path("DERODERO/DATA/BLT_ANM.OBJ").unwrap()).unwrap();
    // Every mask/arm variant of a timed state has the length stackers-core assumes.
    for color in 1..=5u8 {
        for (state, len) in [(4u8, 47), (5, 47), (8, 12), (9, 30), (10, 13)] {
            for mask in 0..16u8 {
                let arms_range = if state == 4 { 0..1 } else { 0..4 };
                for arms in arms_range {
                    // Arms only extend toward an empty side, so they never coexist with a
                    // neighbour bit on that side (right arm 2 / mask 4, left arm 1 / mask 2).
                    if (arms & 2 != 0 && mask & 4 != 0) || (arms & 1 != 0 && mask & 2 != 0) {
                        continue;
                    }
                    let key = CellKey { color, state, mask, arms };
                    let s = bank.cell_script(key).unwrap_or_else(|| panic!("{key:?}"));
                    // `done` is raised by the step after the last frame elapses.
                    assert_eq!(ScriptPlayer::length(&bank, s, 1000), Some(len + 1), "{key:?}");
                }
            }
        }
        for ps in [0u8, 3] {
            let s = bank.piece_script(color, ps).unwrap();
            let mut p = ScriptPlayer::new(s);
            p.step(&bank);
            let f = bank.frame(p.frame).unwrap();
            assert!(f.w > 0 && f.h > 0, "piece colour {color} state {ps}");
            assert!((0..10).contains(&f.page));
        }
    }
}

#[test]
fn every_vab_on_disc_decodes() {
    use stackers_formats::vab::VabHeader;
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let heads: Vec<_> = disc.entries().iter().filter(|e| e.path.ends_with(".VH")).collect();
    assert_eq!(heads.len(), 24, "VAB_SE + VAB_CHR");
    for e in heads {
        let vh = disc.read(e).unwrap();
        let vb = disc.read_path(&e.path.replace(".VH", ".VB")).unwrap();
        let h = VabHeader::parse(&vh).unwrap_or_else(|err| panic!("{}: {err}", e.path));
        // The VAG size table accounts for the whole body (up to sector padding).
        let end = h.vags.last().unwrap().end;
        assert!(end <= vb.len() && vb.len() - end < 2048, "{}: {end} vs {}", e.path, vb.len());
        for p in &h.programs {
            for t in &p.tones {
                let s = h.vag(&vb, t.vag).unwrap_or_else(|err| panic!("{} vag {}: {err}", e.path, t.vag));
                assert!(!s.pcm.is_empty(), "{} vag {}", e.path, t.vag);
            }
        }
    }
}

#[test]
fn sound_tables_point_at_real_tones() {
    use stackers_formats::{sound::SoundTables, vab::VabHeader};
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let t = SoundTables::from_exe(&disc.read_path("SLUS_003.15").unwrap()).unwrap();
    assert_eq!(t.banks[0].name, "DEROMAIN");
    assert_eq!(t.effects.len(), 194);
    let mut missing = vec![];
    for (id, e) in t.effects.iter().enumerate() {
        let (vh, vb) = t.banks[usize::from(e.bank)].vab_paths().expect("VAB bank");
        let h = VabHeader::parse(&disc.read_path(&vh).unwrap()).unwrap();
        let Some(tone) = h.programs.get(usize::from(e.program)).and_then(|p| {
            p.tones.iter().find(|t| (t.min..=t.max).contains(&e.key))
        }) else {
            missing.push(id);
            continue;
        };
        assert!(h.vag(&disc.read_path(&vb).unwrap(), tone.vag).is_ok());
    }
    // A few entries name keys their bank doesn't define (unused effects); none of the
    // in-game ones may be among them.
    eprintln!("effects without a tone: {missing:?}");
    assert!(missing.len() < 16, "{missing:?}");
    for id in [2, 3, 7, 9, 10, 11, 12, 13, 14, 15, 16, 17] {
        assert!(!missing.contains(&id), "effect {id}");
    }
}
