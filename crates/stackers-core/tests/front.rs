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

#[test]
fn chain_reaction_launches_mode_seven() {
    let mut f = Front::new();
    run(&mut f, 10, 0);
    f.step(pad::START);
    run(&mut f, 30, 0);
    f.step(pad::START);
    run(&mut f, 30, 0);
    for _ in 0..3 {
        f.step(pad::DOWN);
        f.step(0);
    }
    f.step(pad::CROSS);
    assert_eq!(f.sounds, [0xA8, 0xA4]);
    let mut frames = 0;
    while !f.launch_chain() {
        f.step(0);
        frames += 1;
        assert!(frames < 400, "never launched");
    }
    assert!(!f.launch_solo());
}

#[test]
fn time_trial_picks_a_course_and_launches_mode_eleven() {
    let mut f = Front::new();
    run(&mut f, 10, 0);
    f.step(pad::START);
    run(&mut f, 30, 0);
    f.step(pad::START);
    run(&mut f, 30, 0);
    for _ in 0..2 {
        f.step(pad::DOWN);
        f.step(0);
    }
    f.step(pad::CROSS);
    assert_eq!(f.sounds, [0xA8, 0xA3]);
    run(&mut f, 26 + 0x3C, 0);
    assert_eq!(f.mode, mode::COURSE);
    f.step(0);
    f.step(0);
    // The list starts on course 2 (the empty field), arrow at x 0x28.
    assert_eq!(f.cursor, 2);
    assert_eq!(f.menu.unwrap().arrow, (0x28, 0x83));
    f.step(pad::UP);
    f.step(0);
    f.step(pad::START);
    assert_eq!(f.course, 1);
    assert_eq!(f.palettes[2], 1);
    let mut frames = 0;
    while !f.launch_trial() {
        f.step(0);
        frames += 1;
        assert!(frames < 400, "never launched");
    }
    assert_eq!(f.course, 1);
    assert!(!f.launch_solo() && !f.launch_chain());
}

#[test]
fn triangle_leaves_the_course_list() {
    let mut f = Front::new();
    run(&mut f, 10, 0);
    f.step(pad::START);
    run(&mut f, 30, 0);
    f.step(pad::START);
    run(&mut f, 30, 0);
    for _ in 0..2 {
        f.step(pad::DOWN);
        f.step(0);
    }
    f.step(pad::CROSS);
    run(&mut f, 26 + 0x3C + 0x20, 0);
    assert_eq!(f.mode, mode::COURSE);
    f.step(pad::TRIANGLE);
    run(&mut f, 0x15, 0);
    assert_eq!(f.mode, mode::MENU);
}
