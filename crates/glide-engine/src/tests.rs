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

/// Speed ripple of a steady roll: the standard deviation of the scroll speed over
/// its mean, in percent, measured once the roll has settled (from the 4th notch to
/// the last). This is the metric SmoothWheelScroll uses; its constant-rate window
/// model measured ~35% at a 150 ms gap with a 200 ms window (see
/// docs/research/prior-art.md). `jitter` varies each gap by up to that fraction,
/// deterministically, because real fingers never roll perfectly evenly.
fn ripple(params: &Params, gap: f64, jitter: f64, hz: f64) -> f64 {
    let dt = 1.0 / hz;
    let mut e = Engine::new(params.clone());
    // Warm up so the engine has learned this pace, as it would in real use.
    roll_on(&mut e, 0.0, 8, gap, dt);

    let notches = 24;
    let mut times = Vec::with_capacity(notches);
    let mut t = 100.0;
    let mut seed = 0x2545_f491_u32;
    for _ in 0..notches {
        times.push(t);
        // xorshift: a fixed, repeatable sequence in [-1, 1).
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let r = seed as f64 / u32::MAX as f64 * 2.0 - 1.0;
        t += gap * (1.0 + jitter * r);
    }

    let (first, last) = (times[3], times[notches - 1]);
    let mut now = 100.0;
    let mut next = 0;
    let mut speeds = Vec::new();
    while now < last {
        while next < notches && times[next] <= now {
            e.on_notch(Axis::Vertical, -1.0, times[next]);
            next += 1;
        }
        e.tick(dt);
        now += dt;
        if now > first {
            speeds.push(-e.velocity(Axis::Vertical));
        }
    }
    let mean = speeds.iter().sum::<f64>() / speeds.len() as f64;
    let var = speeds.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / speeds.len() as f64;
    var.sqrt() / mean * 100.0
}

#[test]
fn steady_rolls_have_little_speed_ripple() {
    let mut params = Preset::MagicMouse.params();
    params.acceleration = 0.0;
    for hz in [60.0, 144.0] {
        for gap_ms in (80..=300).step_by(10) {
            let r = ripple(&params, gap_ms as f64 / 1000.0, 0.0, hz);
            assert!(r < 5.0, "{hz} Hz, {gap_ms} ms gap: ripple {r:.1}%");
        }
    }
}

#[test]
fn uneven_rolls_stay_calmer_than_the_notches() {
    let mut params = Preset::MagicMouse.params();
    params.acceleration = 0.0;
    // With gaps varying by up to ±20%, the speed should vary less than the input does.
    for gap_ms in (80..=300).step_by(20) {
        let r = ripple(&params, gap_ms as f64 / 1000.0, 0.2, 60.0);
        assert!(r < 15.0, "{gap_ms} ms gap with jitter: ripple {r:.1}%");
    }
}

/// Ripple of another tool's model on a perfectly steady roll at 60 Hz, measured the
/// same way. `share(t)` is the cumulative fraction of a notch paid out `t` seconds
/// after it arrived; every notch is paid out independently and the results summed,
/// which is how both reference models work.
fn reference_ripple(gap: f64, share: impl Fn(f64) -> f64) -> f64 {
    let dt = 1.0 / 60.0;
    let notches = 40;
    let (start, end) = (8.0 * gap, (notches - 1) as f64 * gap);
    let mut speeds = Vec::new();
    let mut t = start;
    while t < end {
        let moved: f64 = (0..notches)
            // Notches are picked up on the next frame, as in the real tools.
            .map(|n| (n as f64 * gap / dt - 1e-9).ceil() * dt)
            .filter(|&at| at <= t)
            .map(|at| share(t + dt - at) - share(t - at))
            .sum();
        speeds.push(moved / dt);
        t += dt;
    }
    let mean = speeds.iter().sum::<f64>() / speeds.len() as f64;
    let var = speeds.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / speeds.len() as f64;
    var.sqrt() / mean * 100.0
}

/// Michael Herf's pulse curve as used by SmoothScroll (400 ms, scale 4).
fn pulse_share(t: f64) -> f64 {
    let raw = |x: f64| {
        if x < 1.0 {
            x - (1.0 - (-x).exp())
        } else {
            let start = (-1.0f64).exp();
            start + (1.0 - (-(x - 1.0)).exp()) * (1.0 - start)
        }
    };
    let u = (t / 0.4).clamp(0.0, 1.0);
    raw(u * 4.0) / raw(4.0)
}

/// SmoothWheelScroll's constant-rate window (200 ms).
fn window_share(t: f64) -> f64 {
    (t / 0.2).clamp(0.0, 1.0)
}

/// Prints the ripple table: `cargo test -p glide-engine ripple_table -- --ignored --nocapture`
#[test]
#[ignore]
fn ripple_table() {
    let mut params = Preset::MagicMouse.params();
    params.acceleration = 0.0;
    println!("gap ms | Glide 60 Hz | Glide 144 Hz | Glide ±20% jitter | pulse queue | 200 ms window");
    for gap_ms in (80..=300).step_by(20) {
        let gap = gap_ms as f64 / 1000.0;
        println!(
            "{gap_ms:>6} | {:>10.1}% | {:>11.1}% | {:>16.1}% | {:>10.1}% | {:>12.1}%",
            ripple(&params, gap, 0.0, 60.0),
            ripple(&params, gap, 0.0, 144.0),
            ripple(&params, gap, 0.2, 60.0),
            reference_ripple(gap, pulse_share),
            reference_ripple(gap, window_share),
        );
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
