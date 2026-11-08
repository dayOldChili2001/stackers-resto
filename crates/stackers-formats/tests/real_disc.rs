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

#[test]
fn music_table_matches_the_audio_tracks() {
    use stackers_formats::music::{tracks_from_exe, PREGAP_SECTORS, SECTOR};
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let tracks = tracks_from_exe(&disc.read_path("SLUS_003.15").unwrap()).unwrap();
    assert_eq!(tracks.len(), 11);
    for (i, t) in tracks.iter().enumerate() {
        assert_eq!(usize::from(t.number), i + 2);
        assert_eq!(t.loops, i != 0, "only track 2 plays once");
        let path = TRACK01.replace("Track 01", &format!("Track {:02}", t.number));
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        // Each entry stops one sector short of the end of its track.
        assert_eq!(meta.len() as usize / SECTOR - PREGAP_SECTORS, t.sectors as usize + 1, "{path}");
        assert!(t.byte_range().end <= meta.len() as usize);
    }
}

#[test]
fn solo_screen_records_fit_the_art() {
    use stackers_formats::hud::{self, SoloScreen, Stage};
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let screen = SoloScreen::from_exe(&disc.read_path("SLUS_003.15").unwrap()).unwrap();
    assert_eq!((screen.window.w, screen.window.h), (192, 240));
    // Monitor, then the six 96×16 labels ("Erase Number" … "Course Time").
    assert_eq!((screen.panels[0].w, screen.panels[0].h), (96, 72));
    assert!(screen.panels[1..7].iter().all(|f| (f.w, f.h) == (96, 16)));
    for stage in [Stage::St22, Stage::St23] {
        let bg = Tim::parse(&disc.read_path(&hud::background(stage)).unwrap()).unwrap();
        assert_eq!((bg.width, bg.height, bg.bpp), (320, 240, 16));
        for page in [0, 9, 12, 13, 18, 24] {
            let path = hud::page_file(page, stage).unwrap();
            assert!(disc.read_path(&path).is_ok(), "{path}");
        }
    }
    // The window's see-through middle is exactly field columns 1–6 and rows 6–17 on screen.
    let w_asa = Tim::parse(&disc.read_path(&hud::page_file(24, Stage::St22).unwrap()).unwrap()).unwrap();
    let rgba = w_asa.to_rgba(0);
    let opaque = |x: i32, y: i32| rgba[((y * w_asa.width as i32 + x) * 4 + 3) as usize] != 0;
    let (ox, oy) = hud::FIELD_ORIGIN;
    let (left, right, top, bottom) = (16 + ox, 7 * 16 + ox, 6 * 16 + oy, 18 * 16 + oy);
    assert!(opaque(left - 1, 120) && !opaque(left, 120));
    assert!(!opaque(right - 1, 120) && opaque(right, 120));
    assert!(opaque(60, top - 1) && !opaque(60, top));
    assert!(!opaque(60, bottom - 1) && opaque(60, bottom));
}
