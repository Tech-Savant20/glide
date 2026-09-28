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
    let single = NOTCH * e.params().ease_friction;
    assert_eq!(e.velocity(Axis::Vertical), single);
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
    let impulse = |gap: f64| {
        let mut p = params.clone();
        p.momentum = false;
        let mut e = Engine::new(p);
        e.on_notch(Axis::Vertical, 1.0, 0.0);
        let first = e.velocity(Axis::Vertical);
        e.on_notch(Axis::Vertical, 1.0, gap);
        e.velocity(Axis::Vertical) - first
    };

    let base = NOTCH * params.ease_friction;
    assert_eq!(impulse(window * 2.0), base);
    assert_eq!(impulse(window), base);

    let gaps = [0.1, 0.08, 0.05, 0.02, 0.005];
    let impulses: Vec<f64> = gaps.iter().map(|&g| impulse(g)).collect();
    assert!(impulses.windows(2).all(|w| w[1] > w[0]), "{impulses:?}");
    assert!(impulses[4] <= base * (1.0 + params.acceleration));
}

#[test]
fn velocity_is_capped() {
    let params = Preset::MagicMouse.params();
    let cap = params.max_velocity;
    let mut e = Engine::new(params);
    flick(&mut e, 200, 0.001);
    assert_eq!(e.velocity(Axis::Vertical), -cap);
}

#[test]
fn axes_are_independent() {
    let mut e = Engine::new(Preset::Subtle.params());
    e.on_notch(Axis::Vertical, 1.0, 0.0);
    e.on_notch(Axis::Horizontal, -1.0, 0.0);
    assert_eq!(run_out(&mut e, FRAME).0, [120, -120]);
}
