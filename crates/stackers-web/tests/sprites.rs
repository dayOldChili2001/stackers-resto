//! Runs a game through the sprite mirror using the user's disc. Skipped when the image is absent.

use stackers_core::pad;
use stackers_formats::iso::Disc;
use stackers_web::{Session, Sprites};

const TRACK01: &str = "../../rom/Tecmo Stackers (USA)/Tecmo Stackers (USA) (Track 01).bin";

#[test]
fn draw_lists_stay_on_their_sheets() {
    let Ok(raw) = std::fs::read(TRACK01) else {
        eprintln!("skipping: {TRACK01} not found");
        return;
    };
    let disc = Disc::open(&raw).unwrap();
    let mut sprites = Sprites::new(&disc.read_path("DERODERO/DATA/BLT_ANM.OBJ").unwrap()).unwrap();
    let mut session = Session::new(1234, 0);
    let mut drawn = 0usize;
    let mut pages = std::collections::BTreeSet::new();
    // Drop pieces in a rotating pattern for a while so matches, chains and arms happen.
    for frame in 0..20_000u32 {
        let col = (frame / 400) % 6;
        let held = match frame % 400 {
            0..=40 if col < 3 => pad::LEFT,
            0..=40 if col > 3 => pad::RIGHT,
            41 if frame / 400 % 2 == 0 => pad::CROSS,
            _ => pad::DOWN,
        };
        session.step(held);
        if session.stats()[8] != 0 {
            session = Session::new(frame, 0);
            sprites.reset();
        }
        let list = sprites.field(&session);
        assert_eq!(list.len() % 9, 0);
        for s in list.chunks(9) {
            let [page, clut, u, v, w, h, _x, _y, _flip] = s.try_into().unwrap();
            assert!((0..10).contains(&page), "{s:?}");
            assert!((0..7).contains(&clut), "{s:?}");
            assert!(u >= 0 && v >= 0 && u + w <= 256 && v + h <= 256, "{s:?}");
            pages.insert(page);
            drawn += 1;
        }
    }
    assert!(drawn > 10_000, "{drawn}");
    eprintln!("pages used: {pages:?}");
}
