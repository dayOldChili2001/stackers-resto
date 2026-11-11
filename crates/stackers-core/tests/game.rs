use stackers_core::field::st;
use stackers_core::{pad, ps, Game, ROWS};

const BOTTOM: usize = ROWS - 2;

fn run_until(g: &mut Game, held: u16, limit: usize, mut done: impl FnMut(&Game) -> bool) -> usize {
    for i in 0..limit {
        if done(g) {
            return i;
        }
        g.step(held);
    }
    panic!("condition not reached in {limit} frames (player state {})", g.player.state);
}

#[test]
fn intro_then_first_piece() {
    let mut g = Game::new(1, 0);
    let n = run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    // 0x8F countdown + 1, then states 4 → 11 → spawn.
    assert_eq!(n, 0x8F + 1 + 2);
    // Spawns at y = 88; the controls run in the same frame and take the first 8 px step.
    assert_eq!((g.player.x, g.player.y, g.player.rot), (48, 96, 0));
    assert_eq!(g.stats.pieces, 1);
}

#[test]
fn piece_falls_locks_and_settles_in_column_three() {
    let mut g = Game::new(7, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    let (piv, sat) = (g.player.pivot, g.player.satellite);
    run_until(&mut g, 0, 2000, |g| g.player.state == ps::LANDED);
    assert_eq!(g.player.y, (BOTTOM as i32) * 16);
    run_until(&mut g, 0, 200, |g| g.stats.pieces == 2);
    assert_eq!(g.field.cell(3, BOTTOM).color, piv);
    assert_eq!(g.field.cell(3, BOTTOM - 1).color, sat);
    assert_eq!(g.field.cell(3, BOTTOM).state, st::SETTLED);
}

#[test]
fn soft_drop_is_eight_pixels_per_frame() {
    let mut g = Game::new(7, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    let y0 = g.player.y;
    g.step(pad::DOWN);
    g.step(pad::DOWN);
    assert_eq!(g.player.y - y0, 16);
}

#[test]
fn move_repeats_after_six_then_every_two_frames() {
    let mut g = Game::new(7, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    let mut xs = vec![];
    for _ in 0..12 {
        g.step(pad::LEFT);
        xs.push(g.player.x);
    }
    // Column 3 → 2 immediately, → 1 after 6 frames, then the wall stops it.
    assert_eq!(xs[0], 32);
    assert_eq!(xs[5], 32);
    assert_eq!(xs[6], 16);
    assert_eq!(*xs.last().unwrap(), 16);
}

#[test]
fn rotation_turns_and_kicks_off_the_wall() {
    let mut g = Game::new(7, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    g.step(pad::CIRCLE);
    assert_eq!(g.player.rot, 1, "circle turns clockwise: satellite to the right");
    for _ in 0..10 {
        g.step(0);
    }
    // Push to the right wall (column 6) with the satellite on top.
    g.step(pad::CROSS);
    for _ in 0..10 {
        g.step(0);
    }
    assert_eq!(g.player.rot, 0);
    for _ in 0..30 {
        g.step(pad::RIGHT);
    }
    assert_eq!(g.player.x, 6 * 16);
    for _ in 0..10 {
        g.step(0);
    }
    g.step(pad::CIRCLE);
    assert_eq!(g.player.rot, 1);
    assert_eq!(g.player.x, 5 * 16, "kicked one column left");
}

#[test]
fn game_over_when_spawn_cell_is_filled() {
    let mut g = Game::new(3, 0);
    for r in 6..=BOTTOM {
        g.field.put(3, r, if r % 2 == 0 { 1 } else { 2 });
    }
    run_until(&mut g, 0, 400, |g| g.is_over());
    assert_eq!(g.stats.pieces, 0);
}

#[test]
fn deterministic_for_a_seed() {
    let script = |g: &mut Game| {
        for i in 0..3000u32 {
            let held = match (i / 40) % 5 {
                0 => pad::LEFT,
                1 => pad::DOWN,
                2 => pad::RIGHT | pad::CIRCLE * u16::from(i % 9 == 0),
                3 => pad::CROSS * u16::from(i % 7 == 0),
                _ => pad::DOWN,
            };
            g.step(held);
        }
    };
    let (mut a, mut b) = (Game::new(42, 3), Game::new(42, 3));
    script(&mut a);
    script(&mut b);
    assert_eq!(a.field, b.field);
    assert_eq!(a.stats, b.stats);
    assert!(a.stats.pieces > 5);
}

#[test]
fn gravity_steps_up_at_spawn_once_quota_is_exceeded() {
    // Level 1: quota 48, speed index 2 (func_80039AC4 uses a strict comparison).
    let mut g = Game::new(1, 0);
    g.stats.toward_speedup = 48;
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    assert_eq!((g.speed_index, g.stats.toward_speedup), (2, 48));

    let mut g = Game::new(1, 0);
    g.stats.toward_speedup = 49 + 48;
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    // One step per spawn, even with two quotas' worth banked.
    assert_eq!((g.speed_index, g.stats.toward_speedup), (3, 49));
    assert_eq!(g.level_info().colors, 4);
}

#[test]
fn sounds_for_move_rotate_land_and_chain_step() {
    use stackers_core::se;
    let mut g = Game::new(7, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    g.step(pad::RIGHT);
    assert_eq!(g.sounds, [se::MOVE]);
    g.step(0);
    g.step(pad::CROSS);
    assert_eq!(g.sounds, [se::ROTATE]);
    let mut heard = vec![];
    run_until(&mut g, 0, 2000, |g| {
        heard.extend_from_slice(&g.sounds);
        g.stats.pieces == 2
    });
    // Touchdown only: both halves come to rest in the slots they were locked into, and a
    // blob landing in its own slot is silent (func_8002193C checks the slot was empty).
    assert_eq!(heard.iter().filter(|&&s| s == se::LAND).count(), 1, "{heard:?}");
}

#[test]
fn clock_starts_after_the_intro() {
    let mut g = Game::new(1, 0);
    run_until(&mut g, 0, 400, |g| g.player.state != ps::INTRO);
    assert_eq!(g.hud.time, 0);
    // 60 frames → one second (frames byte resets, seconds byte = 1).
    for _ in 0..60 {
        g.step(0);
    }
    assert_eq!(g.hud.time, 0x100);
    assert_eq!(g.hud.hms(), (0, 0, 1));
}

#[test]
fn soft_drop_scores_100_per_eight_frames() {
    let mut g = Game::new(7, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    for _ in 0..7 {
        g.step(pad::DOWN);
    }
    assert_eq!(g.hud.score, 0);
    g.step(pad::DOWN);
    assert_eq!(g.hud.score, 100);
}

#[test]
fn erase_number_counts_finished_clears() {
    let mut g = Game::new(7, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    let piv = g.player.pivot;
    for r in BOTTOM - 2..=BOTTOM {
        g.field.put(3, r, piv);
    }
    run_until(&mut g, pad::DOWN, 400, |g| g.player.state == ps::CLEARING);
    assert_eq!(g.hud.erased, 0, "counted when the clear animation ends, not at the match");
    run_until(&mut g, 0, 400, |g| g.hud.erased > 0);
    assert!(g.hud.erased >= 4);
    assert_eq!(g.hud.erased, g.stats.cleared);
}

#[test]
fn solo_starts_on_row_three() {
    let g = stackers_core::Game::solo(1);
    assert_eq!(g.rules, stackers_core::Rules::Solo);
    assert_eq!((g.level, g.speed_index), (3, 4));
    assert_eq!(stackers_core::tables::gravity(-1), 0xC0010);
}

#[test]
fn queue_slides_up_to_rest_and_shifts_on_spawn() {
    use stackers_core::game::{QUEUE_REST_Y, QUEUE_START_Y};
    let mut g = Game::new(3, 0);
    assert_eq!(g.queue_y, QUEUE_START_Y);
    g.step(0);
    assert_eq!(g.queue_y, [0x27, 0x40, 0x59]);
    assert_eq!(g.queue_age, 1);
    // The intro (0x8F frames) is longer than the 40 px slide, so play starts at rest.
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    let next = g.queue[1];
    // Spawned this frame: slots took the y of the one below, then moved 1 px.
    assert_eq!(g.queue_y, [0x27, 0x40, 0x59]);
    assert_eq!(g.queue_age, 1);
    run_until(&mut g, pad::DOWN, 2000, |g| g.stats.pieces == 2);
    assert_eq!(g.player.pivot, next.0);
    for _ in 0..0x28 {
        g.step(0);
    }
    assert_eq!(g.queue_y, QUEUE_REST_Y);
}

fn topped_out(seed: u32) -> Game {
    let mut g = Game::solo(seed);
    for r in 6..=BOTTOM {
        g.field.put(3, r, 1 + (r % 2) as u8);
    }
    run_until(&mut g, 0, 400, |g| g.is_over());
    g
}

#[test]
fn game_over_shows_the_result_then_counts_down_to_the_title() {
    use stackers_core::over::{phase, script, se, Outcome};
    let mut g = topped_out(3);
    assert!(g.over.active && g.over.new_record, "erase 0 ≥ record 0");
    assert_eq!(g.record, 0);
    g.step(0);
    assert_eq!(g.over.scripts, [script::NEW_RECORD]);
    let n = run_until(&mut g, 0, 400, |g| g.over.phase == phase::COUNTDOWN);
    assert_eq!(n, 300);
    assert_eq!(g.over.message, script::COUNTDOWN_9);
    assert!(g.sounds.contains(&se::TICK));
    // Ten digits, 120 frames each, then "Game Over" for 200 frames.
    let n = run_until(&mut g, 0, 2000, |g| g.over.phase == phase::GAME_OVER);
    assert_eq!(n, 10 * 120);
    assert_eq!(g.over.message, script::GAME_OVER);
    let n = run_until(&mut g, 0, 400, |g| g.over.outcome.is_some());
    assert_eq!((n, g.over.outcome), (200, Some(Outcome::Title)));
}

#[test]
fn cross_skips_a_digit_and_start_retries() {
    use stackers_core::over::{phase, script, Outcome};
    let mut g = topped_out(4);
    g.record = 5;
    run_until(&mut g, 0, 400, |g| g.over.phase == phase::COUNTDOWN);
    g.step(pad::CROSS);
    assert_eq!(g.over.scripts, [script::COUNTDOWN_9 + 1]);
    g.step(pad::CROSS); // held, not a new press
    assert!(g.over.scripts.is_empty());
    g.step(0);
    g.step(pad::START);
    assert_eq!(g.over.phase, phase::CONTINUE);
    let n = run_until(&mut g, pad::START, 100, |g| g.over.outcome.is_some());
    assert_eq!((n, g.over.outcome), (60, Some(Outcome::Retry)));
}

#[test]
fn burst_throws_every_blob_off_the_field() {
    let mut g = topped_out(5);
    assert!(!g.over.new_record || g.hud.erased >= g.record);
    let before = (6..=BOTTOM).filter(|&r| g.field.cell(3, r).state != st::EMPTY).count();
    assert!(before > 0);
    let y0 = g.field.cell(3, BOTTOM).y;
    g.step(0); // result frame: no burst step yet
    assert_eq!(g.field.cell(3, BOTTOM).y, y0);
    g.step(0);
    assert!(g.field.cell(3, BOTTOM).y <= y0, "starts by hopping up");
    run_until(&mut g, 0, 300, |g| (0..ROWS - 1).all(|r| (1..7).all(|c| g.field.cell(c, r).state == st::EMPTY)));
}

fn playing_solo(seed: u32) -> Game {
    let mut g = Game::solo(seed);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    g
}

#[test]
fn start_is_ignored_during_the_intro() {
    let mut g = Game::solo(1);
    g.step(pad::START);
    assert!(!g.pause.active);
}

#[test]
fn pause_freezes_play_and_start_resumes() {
    let mut g = playing_solo(2);
    g.step(pad::START);
    assert!(g.pause.active);
    let (y, time, queue) = (g.player.y, g.hud.time, g.queue_y);
    for _ in 0..100 {
        g.step(0);
    }
    assert_eq!((g.player.y, g.hud.time, g.queue_y), (y, time, queue));
    g.step(pad::START);
    assert!(!g.pause.active);
    g.step(0);
    assert_ne!(g.hud.time, time, "the clock runs again");
}

#[test]
fn pause_cursor_wraps_and_continue_resumes() {
    use stackers_core::pause::{menu, BRIGHT_OFF, BRIGHT_ON, CLUT_CHOSEN, CLUT_CURSOR, SE_CHOOSE};
    let mut g = playing_solo(3);
    g.step(pad::START);
    g.step(0);
    g.step(pad::UP);
    assert_eq!(g.pause.cursor, 2);
    assert_eq!(g.pause.item_bright, [BRIGHT_OFF, BRIGHT_OFF, BRIGHT_ON]);
    assert_eq!(g.pause.item_clut[2], CLUT_CURSOR);
    g.step(0);
    g.step(pad::DOWN);
    assert_eq!(g.pause.cursor, 0);
    g.step(0);
    // Two new presses on one frame are ignored (the original compares the edge word).
    g.step(pad::DOWN | pad::CROSS);
    assert_eq!((g.pause.cursor, g.pause.menu), (0, menu::SELECT));
    g.step(0);
    g.step(pad::CROSS);
    assert_eq!((g.pause.menu, g.pause.item_clut[0]), (menu::CONTINUE, CLUT_CHOSEN));
    assert!(g.sounds.contains(&SE_CHOOSE));
    g.step(0);
    assert!(!g.pause.active);
}

#[test]
fn quit_asks_first_and_retry_leaves_after_61_frames() {
    use stackers_core::pause::menu;
    use stackers_core::Outcome;
    let mut g = playing_solo(4);
    g.step(pad::START);
    g.step(0);
    g.step(pad::DOWN);
    g.step(0);
    g.step(pad::CROSS);
    assert_eq!(g.pause.menu, menu::CONFIRM);
    g.step(0);
    g.step(pad::TRIANGLE);
    assert_eq!(g.pause.menu, menu::SELECT);
    g.step(0);
    g.step(pad::DOWN);
    assert_eq!(g.pause.cursor, 2);
    g.step(0);
    g.step(pad::CROSS);
    assert_eq!(g.pause.menu, menu::RETRY);
    let n = run_until(&mut g, 0, 100, |g| g.outcome().is_some());
    assert_eq!((n, g.outcome()), (61, Some(Outcome::Retry)));

    let mut g = playing_solo(5);
    g.step(pad::START);
    g.step(0);
    g.step(pad::DOWN);
    g.step(0);
    g.step(pad::CROSS);
    g.step(0);
    g.step(pad::CROSS);
    assert_eq!(g.pause.menu, menu::QUIT);
    run_until(&mut g, 0, 100, |g| g.outcome().is_some());
    assert_eq!(g.outcome(), Some(Outcome::Title));
}

#[test]
fn chain_reaction_pair_glides_down_between_gravity_steps() {
    // Gravity 0x80010: 8 px every 16 frames, drawn 0.5 px per frame (D_80100618).
    let mut g = Game::chain_reaction(3, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    let mut drawn = Vec::new();
    for _ in 0..32 {
        g.step(0);
        drawn.push(g.pair_draw().pivot.1);
    }
    // Never jumps more than a pixel between frames, and keeps falling.
    assert!(drawn.windows(2).all(|w| (0..=1).contains(&(w[1] - w[0]))), "{drawn:?}");
    assert!(drawn[31] > drawn[0]);
    // Drawn y trails the logical y, never leads it.
    assert!(g.pair_draw().pivot.1 <= g.player.y);
}

#[test]
fn chain_reaction_camera_follows_the_falling_pair() {
    let mut g = Game::chain_reaction(3, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    assert_eq!(g.camera.y, -16);
    // Soft drop: once the pair is 0xC8 px down the camera tracks it.
    run_until(&mut g, pad::DOWN, 400, |g| g.player.y > 0x200);
    assert_eq!(g.camera.y, g.pair_draw().pivot.1 - 0xC8);
}

#[test]
fn rotation_eases_the_satellite_round() {
    let mut g = Game::new(5, 0);
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
    g.step(pad::CIRCLE);
    // The press frame still shows the old position (cooldown 8 = a quarter turn back).
    let d = g.pair_draw();
    assert_eq!((d.satellite.0 - d.pivot.0, d.satellite.1 - d.pivot.1), (0, -16));
    for _ in 0..4 {
        g.step(0);
    }
    let d = g.pair_draw();
    let (dx, dy) = (d.satellite.0 - d.pivot.0, d.satellite.1 - d.pivot.1);
    // Halfway through: between up (0, −16) and right (16, 0), still 16 px away.
    assert!(dx > 0 && dy < 0, "{dx},{dy}");
    assert!((dx * dx + dy * dy - 256).abs() < 20);
}

#[test]
fn button_layout_remaps_the_play_but_not_the_pause() {
    // Type A: ✕ turns one way; Type B: □ does the same, ✕ is the bomb button.
    let turn = |layout: u8, button: u16| {
        let mut g = Game::new(5, 0);
        g.layout = layout;
        run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL);
        g.step(button);
        g.player.rot
    };
    let a = turn(0, pad::CROSS);
    assert_ne!(a, turn(0, 0));
    assert_eq!(turn(1, pad::SQUARE), a);
    assert_eq!(turn(1, pad::CROSS), turn(0, 0));
    // Start still pauses under any layout.
    let mut g = Game::solo(5);
    g.layout = 2;
    run_until(&mut g, 0, 400, |g| g.player.state == ps::CONTROL && g.clock_running());
    g.step(pad::START);
    assert!(g.pause.active);
}
