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
    // Touchdown, then both halves land as blobs (one sound per frame at most).
    assert_eq!(heard.iter().filter(|&&s| s == se::LAND).count(), 2, "{heard:?}");
}
