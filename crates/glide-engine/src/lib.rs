//! Scroll physics for Glide.
//!
//! Units are Windows wheel units: one wheel detent ("notch") is [`NOTCH`] = 120.
//! Time is in seconds. The engine never reads a clock; callers pass time in, which
//! keeps it deterministic and testable.
//!
//! The model is a single velocity per axis. Every notch adds an impulse, and the
//! velocity decays exponentially under friction. Slow notches use a high "ease"
//! friction so each one settles quickly; a fast burst of notches (a flick) switches
//! to a low "momentum" friction so the page coasts, like macOS. The impulse is sized
//! so that one notch at rest travels exactly `step` units.

mod presets;

pub use presets::Preset;

/// Wheel units per notch (`WHEEL_DELTA`).
pub const NOTCH: f64 = 120.0;

/// Apple's `UIScrollView.DecelerationRate.normal`: velocity is multiplied by this
/// every millisecond while coasting.
pub const APPLE_DECELERATION_NORMAL: f64 = 0.998;

/// Apple's `UIScrollView.DecelerationRate.fast`.
pub const APPLE_DECELERATION_FAST: f64 = 0.99;

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
    /// Distance one notch travels from rest, in wheel units.
    pub step: f64,
    /// Friction (1/s) for ordinary notches. Higher settles faster.
    pub ease_friction: f64,
    /// Whether a flick switches to momentum friction.
    pub momentum: bool,
    /// Friction (1/s) while coasting after a flick.
    pub momentum_friction: f64,
    /// Notches closer together than this (s) count as part of a flick.
    pub fling_interval: f64,
    /// Notches in a fast burst needed to start momentum.
    pub fling_notches: u32,
    /// Extra impulse multiplier at the fastest notch rate. 0 disables acceleration.
    pub acceleration: f64,
    /// Notches further apart than this (s) get no acceleration.
    pub acceleration_window: f64,
    /// Velocity cap, in wheel units per second.
    pub max_velocity: f64,
    /// Motion slower than this (units/s) ends, and the remaining distance is flushed.
    pub stop_velocity: f64,
}

impl Default for Params {
    fn default() -> Self {
        Preset::MagicMouse.params()
    }
}

#[derive(Clone, Debug, Default)]
struct AxisState {
    velocity: f64,
    friction: f64,
    /// Fractional units computed but not yet emitted.
    carry: f64,
    last_notch: Option<f64>,
    burst: u32,
}

impl AxisState {
    fn halt(&mut self) {
        self.velocity = 0.0;
        self.carry = 0.0;
        self.burst = 0;
    }
}

#[derive(Clone, Debug)]
pub struct Engine {
    params: Params,
    axes: [AxisState; 2],
}

impl Engine {
    pub fn new(params: Params) -> Self {
        Self {
            params,
            axes: Default::default(),
        }
    }

    pub fn params(&self) -> &Params {
        &self.params
    }

    /// Replaces the tuning. Motion already in flight keeps its current friction.
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

        if s.velocity != 0.0 && s.velocity.signum() != dir {
            s.halt();
        }

        // A clock that went backwards says nothing about notch rate.
        let interval = s.last_notch.map(|t| now - t).filter(|dt| *dt >= 0.0);
        s.last_notch = Some(now);

        let fast = interval.is_some_and(|dt| dt < p.fling_interval);
        s.burst = if fast { s.burst + 1 } else { 1 };

        if p.momentum && s.burst >= p.fling_notches {
            s.friction = p.momentum_friction;
        } else if s.velocity == 0.0 {
            s.friction = p.ease_friction;
        }

        let boost = interval.map_or(0.0, |dt| {
            ((p.acceleration_window - dt) / p.acceleration_window).clamp(0.0, 1.0)
        });
        let factor = 1.0 + p.acceleration * boost * boost;

        // Sized against ease friction so one notch from rest travels exactly `step`.
        s.velocity += notches * p.step * factor * p.ease_friction;
        s.velocity = s.velocity.clamp(-p.max_velocity, p.max_velocity);
    }

    /// Advances time by `dt` seconds and returns the whole wheel units to emit on
    /// each axis, indexed by [`Axis`].
    pub fn tick(&mut self, dt: f64) -> [i32; 2] {
        let stop_velocity = self.params.stop_velocity;
        self.axes.each_mut().map(|s| {
            if s.velocity == 0.0 {
                return 0;
            }
            // Exact integral of v0 * exp(-k t) over the frame, so the total distance
            // does not depend on the frame rate.
            let decay = (-s.friction * dt).exp();
            let mut travel = s.velocity / s.friction * (1.0 - decay);
            s.velocity *= decay;

            let stopping = s.velocity.abs() < stop_velocity;
            if stopping {
                travel += s.velocity / s.friction;
            }

            let total = s.carry + travel;
            if stopping {
                s.halt();
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
        self.axes.iter().any(|s| s.velocity != 0.0)
    }

    /// Current velocity in wheel units per second.
    pub fn velocity(&self, axis: Axis) -> f64 {
        self.axes[axis as usize].velocity
    }
}

#[cfg(test)]
mod tests;
