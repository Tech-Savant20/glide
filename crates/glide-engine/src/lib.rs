//! Scroll physics for Glide.
//!
//! Units are Windows wheel units: one wheel detent ("notch") is [`NOTCH`] = 120.
//! Time is in seconds. The engine never reads a clock; callers pass time in, which
//! keeps it deterministic and testable.
//!
//! Each axis tracks the distance it still owes (every notch adds `step`) and a
//! velocity, and moves in one of two phases:
//!
//! - **Rolling**: while notches keep arriving, the engine reads their spacing as the
//!   speed of the user's finger and moves at that steady speed. Replaying each notch
//!   as its own ease-out would make the speed spike and sag with every notch, which
//!   reads as jitter.
//! - **Gliding**: once the next notch is overdue, the rest runs out on an
//!   exponential ease-out, with no jump in speed at the handover. After a flick it
//!   coasts at Apple's deceleration rate instead (momentum).
//!
//! The distance owed is always emitted exactly, whatever the frame rate.

mod presets;

pub use presets::Preset;

/// Wheel units per notch (`WHEEL_DELTA`).
pub const NOTCH: f64 = 120.0;

/// Apple's `UIScrollView.DecelerationRate.normal`: velocity is multiplied by this
/// every millisecond while coasting.
pub const APPLE_DECELERATION_NORMAL: f64 = 0.998;

/// Apple's `UIScrollView.DecelerationRate.fast`.
pub const APPLE_DECELERATION_FAST: f64 = 0.99;

/// While rolling, speed is planned so the next notch may arrive this much later than
/// the last gap (as a multiple) without the motion sagging.
const ROLL_SLACK: f64 = 1.25;

/// Time constant (s) for easing between rolling speeds, so a change in finger
/// speed never shows up as a one-frame jump.
const SPEED_SMOOTHING: f64 = 0.03;

/// How much each new notch gap moves the rolling pace estimate (0..1).
const GAP_SMOOTHING: f64 = 0.3;

/// Converts a per-millisecond velocity multiplier into a continuous friction
/// coefficient `k` (1/s), where `v(t) = v0 * exp(-k t)`.
pub fn friction_from_per_ms(rate: f64) -> f64 {
    -rate.ln() * 1000.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Vertical = 0,
    Horizontal = 1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    /// Distance one notch travels, in wheel units.
    pub step: f64,
    /// Friction (1/s) of the ease-out that ends every scroll. Higher settles faster.
    pub ease_friction: f64,
    /// Notches closer together than this (s) are one continuous roll of the wheel.
    pub roll_gap: f64,
    /// Starting guess (s) for how far apart the user's notches are when rolling.
    /// A notch from rest is paced for this gap; the engine then learns the real one.
    pub first_gap: f64,
    /// Whether a flick keeps coasting.
    pub momentum: bool,
    /// Friction (1/s) while coasting after a flick.
    pub momentum_friction: f64,
    /// Notches closer together than this (s) count as part of a flick.
    pub fling_interval: f64,
    /// Notches in a fast burst needed to start momentum.
    pub fling_notches: u32,
    /// Extra distance multiplier at the fastest notch rate. 0 disables acceleration.
    pub acceleration: f64,
    /// Notches further apart than this (s) get no acceleration.
    pub acceleration_window: f64,
    /// Velocity cap, in wheel units per second.
    pub max_velocity: f64,
    /// A glide slower than this (units/s) ends, and the remaining distance is flushed.
    pub stop_velocity: f64,
}

impl Default for Params {
    fn default() -> Self {
        Preset::MagicMouse.params()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    /// Moving at the finger's speed, estimated from recent notch gaps. `measured`
    /// is false while the gap is still the guessed usual pace.
    Rolling {
        gap: f64,
        measured: bool,
    },
    Gliding,
}

#[derive(Clone, Debug, Default)]
struct AxisState {
    phase: Phase,
    /// Distance still to emit. Always has the sign of the motion.
    owed: f64,
    velocity: f64,
    /// The finger's speed, estimated from the last notch gap.
    finger: f64,
    since_notch: f64,
    /// Fractional units computed but not yet emitted.
    carry: f64,
    last_notch: Option<f64>,
    burst: u32,
    /// The current roll is a flick, so it coasts when the notches stop.
    coast: bool,
}

impl AxisState {
    fn halt(&mut self) {
        *self = AxisState {
            last_notch: self.last_notch,
            ..AxisState::default()
        };
    }

    /// Advances one frame, returning the (fractional) distance moved.
    fn advance(&mut self, dt: f64, p: &Params) -> f64 {
        self.since_notch += dt;
        let dir = self.owed.signum();

        if let Phase::Rolling { gap, .. } = self.phase {
            let due = ROLL_SLACK * gap - self.since_notch;
            if due > 0.0 {
                // Aim to arrive, when the next notch is due, with exactly an
                // ease-out's worth of distance left. While rolling steadily this
                // target is constant.
                let target = (self.owed / (due + 1.0 / p.ease_friction))
                    .clamp(-p.max_velocity, p.max_velocity);
                return self.cruise(target, dt);
            }
            // The next notch is overdue: the finger has stopped.
            if self.coast {
                let speed = self.velocity.abs().max(self.finger.abs());
                self.velocity = dir * speed;
                self.owed = dir * self.owed.abs().max(speed / p.momentum_friction);
            }
            self.phase = Phase::Gliding;
        }

        // Glide: an exponential whose rate is picked so that, from the current
        // velocity, it covers exactly the distance owed. Speed is therefore
        // continuous when rolling hands over.
        let settle = if self.coast {
            p.momentum_friction
        } else {
            p.ease_friction
        };
        if self.velocity.signum() != dir || self.velocity == 0.0 {
            self.velocity = (self.owed * settle).clamp(-p.max_velocity, p.max_velocity);
        }
        let k = (self.velocity / self.owed).abs();
        if !k.is_finite() {
            self.halt();
            return 0.0;
        }
        if k < settle * 0.999 {
            // More owed than this speed can ease out (the speed cap was hit):
            // hold speed until the remainder fits.
            let target = (self.owed * settle).clamp(-p.max_velocity, p.max_velocity);
            return self.cruise(target, dt);
        }
        let decay = (-k * dt).exp();
        let travel = self.velocity / k * (1.0 - decay);
        self.velocity *= decay;
        self.owed -= travel;

        if self.velocity.abs() < p.stop_velocity {
            let rest = self.owed;
            self.halt();
            return travel + rest;
        }
        travel
    }

    /// Eases the velocity towards `target` and moves linearly, never past what is owed.
    fn cruise(&mut self, target: f64, dt: f64) -> f64 {
        let blend = 1.0 - (-dt / SPEED_SMOOTHING).exp();
        self.velocity += (target - self.velocity) * blend;
        let mut travel = self.velocity * dt;
        if travel.abs() > self.owed.abs() || travel.signum() != self.owed.signum() {
            travel = self.owed;
        }
        self.owed -= travel;
        travel
    }
}

#[derive(Clone, Debug)]
pub struct Engine {
    params: Params,
    axes: [AxisState; 2],
    /// How far apart this user's notches usually are when rolling (s), learned
    /// as they scroll.
    usual_gap: f64,
}

impl Engine {
    pub fn new(params: Params) -> Self {
        Self {
            usual_gap: params.first_gap,
            params,
            axes: Default::default(),
        }
    }

    pub fn params(&self) -> &Params {
        &self.params
    }

    /// Replaces the tuning. Motion already in flight carries on.
    pub fn set_params(&mut self, params: Params) {
        self.params = params;
    }

    /// Feeds wheel input. `notches` is signed (positive is up/right, as in
    /// `WM_MOUSEWHEEL`) and may be more than one for a coalesced event.
    /// `now` is the event time in seconds.
    pub fn on_notch(&mut self, axis: Axis, notches: f64, now: f64) {
        if notches == 0.0 {
            return;
        }
        let p = &self.params;
        let s = &mut self.axes[axis as usize];
        let dir = notches.signum();

        if s.phase != Phase::Idle && s.owed.signum() != dir {
            s.halt();
        }

        // A clock that went backwards says nothing about notch rate.
        let gap = s.last_notch.map(|t| now - t).filter(|dt| *dt >= 0.0);
        s.last_notch = Some(now);

        let fast = gap.is_some_and(|dt| dt < p.fling_interval);
        s.burst = if fast { s.burst + 1 } else { 1 };
        s.coast = p.momentum && s.burst >= p.fling_notches;

        let boost = gap.map_or(0.0, |dt| {
            ((p.acceleration_window - dt) / p.acceleration_window).clamp(0.0, 1.0)
        });
        s.owed += notches * p.step * (1.0 + p.acceleration * boost * boost);
        s.since_notch = 0.0;

        // A notch from rest rolls as if the wheel were turning at the user's usual
        // pace. If it was a lone notch it simply eases out; if more follow, they
        // arrive before the ease-out starts and the roll speeds up smoothly.
        // Giving the first notch a quick ease-out of its own instead front-loads
        // the motion, so it would surge, sag, then pick up again at the second.
        let (gap, measured) = match gap {
            Some(gap) if gap < p.roll_gap => {
                let gap = gap.max(0.001);
                if gap < p.fling_interval {
                    // A flick: react at once.
                    (gap, true)
                } else {
                    self.usual_gap = 0.8 * self.usual_gap + 0.2 * gap;
                    // Nobody rolls a wheel perfectly evenly, so average the gap over
                    // the roll rather than reacting fully to each one.
                    match s.phase {
                        Phase::Rolling {
                            gap: previous,
                            measured: true,
                        } => (previous + GAP_SMOOTHING * (gap - previous), true),
                        _ => (gap, true),
                    }
                }
            }
            _ => (self.usual_gap.clamp(p.fling_interval, p.roll_gap), false),
        };
        let was_idle = s.phase == Phase::Idle;
        s.phase = Phase::Rolling { gap, measured };
        // Speed that leaves exactly an ease-out's worth of distance when the next
        // notch is due, so steady rolling moves at step / gap.
        s.finger = (s.owed / (ROLL_SLACK * gap + 1.0 / p.ease_friction))
            .clamp(-p.max_velocity, p.max_velocity);
        if was_idle {
            s.velocity = s.finger;
        }
    }

    /// Advances time by `dt` seconds and returns the whole wheel units to emit on
    /// each axis, indexed by [`Axis`].
    pub fn tick(&mut self, dt: f64) -> [i32; 2] {
        let p = &self.params;
        self.axes.each_mut().map(|s| {
            if s.phase == Phase::Idle {
                return 0;
            }
            let total = s.carry + s.advance(dt, p);
            if s.phase == Phase::Idle {
                // Finished: round the last fraction so totals come out exact.
                s.carry = 0.0;
                total.round() as i32
            } else {
                let whole = total.trunc();
                s.carry = total - whole;
                whole as i32
            }
        })
    }

    /// Cancels all motion, for example when the user clicks.
    pub fn stop(&mut self) {
        for s in &mut self.axes {
            s.halt();
        }
    }

    pub fn is_active(&self) -> bool {
        self.axes.iter().any(|s| s.phase != Phase::Idle)
    }

    /// Current velocity in wheel units per second.
    pub fn velocity(&self, axis: Axis) -> f64 {
        self.axes[axis as usize].velocity
    }
}

#[cfg(test)]
mod tests;
