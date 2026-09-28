use crate::{friction_from_per_ms, Params, APPLE_DECELERATION_FAST, APPLE_DECELERATION_NORMAL, NOTCH};

/// Built-in tunings shown on the settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// A Magic Mouse on macOS: eased notches, momentum on flicks.
    MagicMouse,
    /// A MacBook trackpad: lighter, flicks coast more readily.
    Trackpad,
    /// Smooth but restrained: no momentum, little acceleration.
    Subtle,
    /// Short glides that stop quickly.
    Snappy,
}

impl Preset {
    pub const ALL: [Preset; 4] = [
        Preset::MagicMouse,
        Preset::Trackpad,
        Preset::Subtle,
        Preset::Snappy,
    ];

    pub fn params(self) -> Params {
        let apple_normal = friction_from_per_ms(APPLE_DECELERATION_NORMAL);
        match self {
            Preset::MagicMouse => Params {
                step: NOTCH,
                ease_friction: 12.0,
                roll_gap: 0.4,
                first_gap: 0.2,
                momentum: true,
                momentum_friction: apple_normal,
                fling_interval: 0.045,
                fling_notches: 3,
                acceleration: 2.0,
                acceleration_window: 0.12,
                max_velocity: 40.0 * NOTCH,
                stop_velocity: 20.0,
            },
            Preset::Trackpad => Params {
                step: NOTCH,
                ease_friction: 9.0,
                roll_gap: 0.45,
                first_gap: 0.2,
                momentum: true,
                momentum_friction: apple_normal,
                fling_interval: 0.06,
                fling_notches: 2,
                acceleration: 3.0,
                acceleration_window: 0.15,
                max_velocity: 50.0 * NOTCH,
                stop_velocity: 20.0,
            },
            Preset::Subtle => Params {
                step: NOTCH,
                ease_friction: 18.0,
                roll_gap: 0.35,
                first_gap: 0.18,
                momentum: false,
                momentum_friction: apple_normal,
                fling_interval: 0.045,
                fling_notches: 3,
                acceleration: 0.5,
                acceleration_window: 0.1,
                max_velocity: 40.0 * NOTCH,
                stop_velocity: 20.0,
            },
            Preset::Snappy => Params {
                step: NOTCH,
                ease_friction: 24.0,
                roll_gap: 0.3,
                first_gap: 0.15,
                momentum: true,
                momentum_friction: friction_from_per_ms(APPLE_DECELERATION_FAST),
                fling_interval: 0.04,
                fling_notches: 3,
                acceleration: 1.5,
                acceleration_window: 0.1,
                max_velocity: 50.0 * NOTCH,
                stop_velocity: 30.0,
            },
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Preset::MagicMouse => "macOS Magic Mouse",
            Preset::Trackpad => "macOS Trackpad",
            Preset::Subtle => "Subtle",
            Preset::Snappy => "Snappy",
        }
    }
}
