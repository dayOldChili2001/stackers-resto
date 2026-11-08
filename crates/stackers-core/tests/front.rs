//! The title → menu → Insane Mode path (`stackers_core::front`).
use stackers_core::front::{mode, Front, Music};
use stackers_core::pad;

fn run(f: &mut Front, frames: u32, held: u16) {
    for _ in 0..frames {
        f.step(held);
    }
}

#[test]
fn title_music_starts_after_the_tecmo_logo() {
    let mut f = Front::new();
    let mut started = None;
    for frame in 0..400 {
        f.step(0);
        if f.music == Some(Music::Play(0)) {
            started = Some(frame);
        }
    }
    // 4 blank frames, the init frame, then 0x12C logo frames.
    assert_eq!(started, Some(4 + 0x12C));
    assert_eq!(f.mode, mode::TITLE);
}

#[test]
fn start_down_cross_launches_solo() {
    let mut f = Front::new();
    run(&mut f, 10, 0);
    f.step(pad::START); // skip the logo
    run(&mut f, 30, 0);
    f.step(pad::START); // leave the title
    run(&mut f, 30, 0);
    assert_eq!(f.mode, mode::MENU);
    f.step(pad::DOWN);
    assert_eq!(f.cursor, 1);
    assert_eq!(f.sounds, [0xA7]);
    f.step(0);
    f.step(pad::CROSS);
    assert_eq!(f.sounds, [0xA8, 0xA2]);
    let mut frames = 0;
    while !f.launch_solo() {
        f.step(0);
        frames += 1;
        assert!(frames < 400, "never launched");
    }
    // Arrow 26 frames, exit 60, mode 9: init 1, wait 64, then 2 + 1 + 4.
    assert_eq!(frames, 26 + 0x3C + 1 + 0x40 + 2 + 1 + 4);
}

#[test]
fn unported_modes_come_back_to_the_menu() {
    let mut f = Front::new();
    run(&mut f, 10, 0);
    f.step(pad::START);
    run(&mut f, 30, 0);
    f.step(pad::START);
    run(&mut f, 30, 0);
    f.step(pad::CROSS); // Arcade
    run(&mut f, 200, 0);
    assert_eq!(f.mode, mode::MENU);
    assert!(!f.launch_solo());
}
