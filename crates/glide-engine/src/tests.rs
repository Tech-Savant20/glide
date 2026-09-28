use super::*;

const FRAME: f64 = 1.0 / 120.0;

/// Ticks until motion ends, returning the total emitted per axis and the time taken.
fn run_out(engine: &mut Engine, dt: f64) -> ([i64; 2], f64) {
    let mut total = [0i64; 2];
    let mut elapsed = 0.0;
    while engine.is_active() {
        let out = engine.tick(dt);
        total[0] += out[0] as i64;
        total[1] += out[1] as i64;
        elapsed += dt;
        assert!(elapsed < 60.0, "motion never stopped");
    }
    (total, elapsed)
}

fn flick(engine: &mut Engine, notches: u32, gap: f64) {
    for i in 0..notches {
        engine.on_notch(Axis::Vertical, -1.0, i as f64 * gap);
    }
}

#[test]
fn apple_normal_rate_is_about_two_per_second() {
    let k = friction_from_per_ms(APPLE_DECELERATION_NORMAL);
    assert!((k - 2.002).abs() < 1e-3, "k = {k}");
}

#[test]
fn single_notch_travels_exactly_one_step_for_every_preset() {
    for preset in Preset::ALL {
        let mut e = Engine::new(preset.params());
        e.on_notch(Axis::Vertical, 1.0, 0.0);
        let (total, _) = run_out(&mut e, FRAME);
        assert_eq!(total, [120, 0], "{preset:?}");
    }
}

#[test]
fn single_notch_settles_quickly() {
    let mut e = Engine::new(Preset::MagicMouse.params());
    e.on_notch(Axis::Vertical, 1.0, 0.0);
    let (_, elapsed) = run_out(&mut e, FRAME);
    assert!(elapsed < 0.5, "took {elapsed}s");
}

#[test]
fn total_distance_does_not_depend_on_frame_rate() {
    let mut totals = Vec::new();
    for dt in [1.0 / 60.0, 1.0 / 144.0, 1.0 / 240.0, 0.001] {
        let mut e = Engine::new(Preset::MagicMouse.params());
        e.on_notch(Axis::Vertical, -3.0, 0.0);
        totals.push(run_out(&mut e, dt).0);
    }
    assert!(totals.iter().all(|t| *t == [-360, 0]), "{totals:?}");
}

#[test]
fn irregular_frames_still_sum_exactly() {
    let mut e = Engine::new(Preset::Subtle.params());
    e.on_notch(Axis::Vertical, 2.0, 0.0);
    let mut total = 0i64;
    let frames = [0.004, 0.016, 0.009, 0.033, 0.001, 0.02];
    let mut i = 0;
    while e.is_active() {
        total += e.tick(frames[i % frames.len()])[0] as i64;
        i += 1;
    }
    assert_eq!(total, 240);
}

#[test]
fn flick_coasts_at_apple_deceleration() {
    let mut e = Engine::new(Preset::MagicMouse.params());
    flick(&mut e, 4, 0.01);
    e.tick(0.05);
    let before = e.velocity(Axis::Vertical);
    e.tick(0.001);
    let ratio = e.velocity(Axis::Vertical) / before;
    assert!((ratio - APPLE_DECELERATION_NORMAL).abs() < 1e-9, "ratio {ratio}");
}

#[test]
fn flick_travels_further_than_the_same_notches_slowly() {
    let mut fast = Engine::new(Preset::MagicMouse.params());
    flick(&mut fast, 4, 0.01);
    let (fast_total, _) = run_out(&mut fast, FRAME);

    let mut slow = Engine::new(Preset::MagicMouse.params());
    let mut slow_total = 0i64;
    for i in 0..4 {
        slow.on_notch(Axis::Vertical, -1.0, i as f64);
        slow_total += run_out(&mut slow, FRAME).0[0];
    }

    assert_eq!(slow_total, -480);
    assert!(fast_total[0] < slow_total * 3, "fast {fast_total:?}");
}

#[test]
fn momentum_off_never_coasts() {
    let mut params = Preset::MagicMouse.params();
    params.momentum = false;
    params.acceleration = 0.0;
    let mut e = Engine::new(params);
    flick(&mut e, 6, 0.01);
    let (total, _) = run_out(&mut e, FRAME);
    assert_eq!(total, [-720, 0]);
}

#[test]
fn opposite_notch_cancels_motion() {
    let mut e = Engine::new(Preset::MagicMouse.params());
    flick(&mut e, 4, 0.01);
    e.tick(0.1);
    assert!(e.velocity(Axis::Vertical) < 0.0);

    e.on_notch(Axis::Vertical, 1.0, 5.0);
    assert!(e.velocity(Axis::Vertical) > 0.0);
    assert_eq!(run_out(&mut e, FRAME).0, [120, 0]);
}

#[test]
fn stop_clears_everything() {
    let mut e = Engine::new(Preset::MagicMouse.params());
    flick(&mut e, 4, 0.01);
    e.on_notch(Axis::Horizontal, 1.0, 0.0);
    e.tick(FRAME);
    e.stop();
    assert!(!e.is_active());
    assert_eq!(e.tick(FRAME), [0, 0]);
}

#[test]
fn acceleration_increases_with_notch_rate() {
    let params = Preset::MagicMouse.params();
    let window = params.acceleration_window;
    // Total distance of two notches `gap` apart.
    let distance = |gap: f64| {
        let mut p = params.clone();
        p.momentum = false;
        let mut e = Engine::new(p);
        e.on_notch(Axis::Vertical, 1.0, 0.0);
        e.on_notch(Axis::Vertical, 1.0, gap);
        run_out(&mut e, FRAME).0[0]
    };

    assert_eq!(distance(window * 2.0), 240);
    assert_eq!(distance(window), 240);

    let gaps = [0.1, 0.08, 0.05, 0.02, 0.005];
    let distances: Vec<i64> = gaps.iter().map(|&g| distance(g)).collect();
    assert!(distances.windows(2).all(|w| w[1] > w[0]), "{distances:?}");
    assert!(distances[4] as f64 <= 120.0 * (2.0 + params.acceleration));
}

#[test]
fn velocity_is_capped() {
    let params = Preset::MagicMouse.params();
    let cap = params.max_velocity;
    let mut e = Engine::new(params);
    flick(&mut e, 200, 0.001);
    while e.is_active() {
        e.tick(FRAME);
        assert!(e.velocity(Axis::Vertical).abs() <= cap);
    }
}

/// Rolls the wheel at a steady `gap` for `notches` notches with frames of `dt`, the
/// way input really interleaves: notches land between frames. Returns every
/// frame's output and the frame index of the last notch.
fn roll(params: Params, notches: u32, gap: f64, dt: f64) -> (Vec<i32>, usize) {
    roll_on(&mut Engine::new(params), 0.0, notches, gap, dt)
}

fn roll_on(e: &mut Engine, start: f64, notches: u32, gap: f64, dt: f64) -> (Vec<i32>, usize) {
    let mut out = Vec::new();
    let mut t = 0.0;
    let mut sent = 0;
    let mut last_notch_frame = 0;
    while sent < notches || e.is_active() {
        while sent < notches && sent as f64 * gap <= t {
            e.on_notch(Axis::Vertical, -1.0, start + sent as f64 * gap);
            sent += 1;
            last_notch_frame = out.len();
        }
        out.push(-e.tick(dt)[0]);
        t += dt;
    }
    (out, last_notch_frame)
}

#[test]
fn steady_rolling_moves_at_a_steady_speed() {
    let mut params = Preset::MagicMouse.params();
    params.acceleration = 0.0;
    // About 5.5 notches a second: the speed from the Scroll Lab report that showed
    // a sawtooth under the old per-notch model.
    let gap = 0.183;
    let (out, last) = roll(params, 12, gap, 1.0 / 60.0);

    let steady = &out[(4.0 * gap * 60.0) as usize..last];
    let expected = 120.0 / gap / 60.0;
    for &px in steady {
        let error = (px as f64 - expected).abs() / expected;
        assert!(error < 0.15, "frame moved {px}, expected ~{expected:.1}: {steady:?}");
    }
}

#[test]
fn starting_a_roll_does_not_surge_and_sag() {
    let mut params = Preset::MagicMouse.params();
    params.acceleration = 0.0;
    // Gaps from the second Scroll Lab report, where every roll began with a surge
    // from the first notch, a sag, then a ramp back up.
    for gap in [0.12, 0.217, 0.27] {
        let mut e = Engine::new(params.clone());
        // The first roll teaches the engine this user's pace...
        roll_on(&mut e, 0.0, 8, gap, 1.0 / 60.0);
        // ...so the next one only ever speeds up until it reaches that pace.
        let (out, last) = roll_on(&mut e, 100.0, 8, gap, 1.0 / 60.0);
        let rising = &out[..last];
        assert!(
            rising.windows(2).all(|w| w[1] >= w[0] - 1),
            "gap {gap}: speed dipped: {rising:?}"
        );
        let steady = 120.0 / gap / 60.0;
        assert!(rising[0] as f64 >= 0.4 * steady, "gap {gap}: slow start: {rising:?}");
    }
}

/// Prints per-frame output for eyeballing: `cargo test -p glide-engine profile -- --ignored --nocapture`
#[test]
#[ignore]
fn profile() {
    let mut params = Preset::MagicMouse.params();
    params.acceleration = 0.0;
    for gap in [0.12, 0.183, 0.27] {
        println!("gap {gap}: {:?}", roll(params.clone(), 6, gap, 1.0 / 60.0).0);
    }
    let mut e = Engine::new(params);
    e.on_notch(Axis::Vertical, -1.0, 0.0);
    let mut single = Vec::new();
    while e.is_active() {
        single.push(-e.tick(1.0 / 60.0)[0]);
    }
    println!("single notch: {single:?}");
}

#[test]
fn rolling_eases_out_without_a_jump() {
    let mut params = Preset::MagicMouse.params();
    params.acceleration = 0.0;
    let (out, last) = roll(params, 8, 0.12, 1.0 / 60.0);
    let tail = &out[last..];
    // After the last notch the speed only ever falls, and it ends gently.
    assert!(tail.windows(2).all(|w| w[1] <= w[0] + 1), "{tail:?}");
    assert!(*tail.last().unwrap() <= 3, "{tail:?}");
    assert_eq!(out.iter().map(|&v| v as i64).sum::<i64>(), 8 * 120);
}

#[test]
fn rolling_totals_are_exact_at_any_frame_rate() {
    let mut params = Preset::Trackpad.params();
    params.momentum = false;
    params.acceleration = 0.0;
    for dt in [1.0 / 60.0, 1.0 / 144.0, 1.0 / 240.0] {
        let (out, _) = roll(params.clone(), 9, 0.07, dt);
        assert_eq!(out.iter().map(|&v| v as i64).sum::<i64>(), 9 * 120, "dt {dt}");
    }
}

#[test]
fn axes_are_independent() {
    let mut e = Engine::new(Preset::Subtle.params());
    e.on_notch(Axis::Vertical, 1.0, 0.0);
    e.on_notch(Axis::Horizontal, -1.0, 0.0);
    assert_eq!(run_out(&mut e, FRAME).0, [120, -120]);
}
