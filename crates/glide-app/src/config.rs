//! `%APPDATA%\Glide\config.toml`: a preset plus optional fine-tuning, reloaded
//! whenever the file is saved.

use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::{fs, io};

use glide_engine::{friction_from_per_ms, Params, Preset, NOTCH};
use glide_win::Options;
use serde::Deserialize;

/// A single notch settles when 95% of its distance has been covered: e^(-k t) = 1/20.
const SETTLE: f64 = 2.995_732_273_553_991; // ln(20)

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub preset: PresetName,
    pub natural: bool,
    pub smooth_hires: bool,
    pub tuning: Tuning,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PresetName {
    #[default]
    MagicMouse,
    Trackpad,
    Subtle,
    Snappy,
}

impl PresetName {
    pub fn preset(self) -> Preset {
        match self {
            PresetName::MagicMouse => Preset::MagicMouse,
            PresetName::Trackpad => Preset::Trackpad,
            PresetName::Subtle => Preset::Subtle,
            PresetName::Snappy => Preset::Snappy,
        }
    }
}

/// User-facing knobs. Each one left unset keeps the preset's value.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tuning {
    pub distance: Option<f64>,
    pub glide_ms: Option<f64>,
    pub momentum: Option<bool>,
    pub deceleration: Option<f64>,
    pub flick_gap_ms: Option<f64>,
    pub flick_notches: Option<u32>,
    pub acceleration: Option<f64>,
    pub acceleration_window_ms: Option<f64>,
    pub max_speed: Option<f64>,
}

impl Config {
    pub fn params(&self) -> Params {
        let mut p = self.preset.preset().params();
        let t = &self.tuning;
        if let Some(v) = t.distance {
            p.step = v.max(0.05) * NOTCH;
        }
        if let Some(v) = t.glide_ms {
            p.ease_friction = SETTLE * 1000.0 / v.max(10.0);
        }
        if let Some(v) = t.momentum {
            p.momentum = v;
        }
        if let Some(v) = t.deceleration {
            p.momentum_friction = friction_from_per_ms(v.clamp(0.9, 0.9999));
        }
        if let Some(v) = t.flick_gap_ms {
            p.fling_interval = v.max(1.0) / 1000.0;
        }
        if let Some(v) = t.flick_notches {
            p.fling_notches = v.max(1);
        }
        if let Some(v) = t.acceleration {
            p.acceleration = v.max(0.0);
        }
        if let Some(v) = t.acceleration_window_ms {
            p.acceleration_window = v.max(1.0) / 1000.0;
        }
        if let Some(v) = t.max_speed {
            p.max_velocity = v.max(1.0) * NOTCH;
        }
        p
    }

    pub fn options(&self) -> Options {
        Options {
            natural: self.natural,
            smooth_hires: self.smooth_hires,
        }
    }

    /// One line describing the effective tuning, printed after every reload.
    pub fn summary(&self) -> String {
        let p = self.params();
        format!(
            "{} | distance {:.2} | glide {:.0} ms | momentum {} (deceleration {:.4}, flick {} notches < {:.0} ms) | acceleration {:.2} within {:.0} ms | max {:.0} notches/s{}",
            self.preset.preset().name(),
            p.step / NOTCH,
            SETTLE * 1000.0 / p.ease_friction,
            if p.momentum { "on" } else { "off" },
            (-p.momentum_friction / 1000.0).exp(),
            p.fling_notches,
            p.fling_interval * 1000.0,
            p.acceleration,
            p.acceleration_window * 1000.0,
            p.max_velocity / NOTCH,
            if self.natural { " | natural" } else { "" },
        )
    }
}

pub fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map_or_else(|| PathBuf::from("."), PathBuf::from);
    base.join("Glide").join("config.toml")
}

/// Reads the config, writing the commented default file first if there is none.
pub fn load_or_create(path: &Path) -> Result<Config, String> {
    if !path.exists() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::write(path, DEFAULT_FILE).map_err(|e| e.to_string())?;
    }
    load(path)
}

pub fn load(path: &Path) -> Result<Config, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

pub fn modified(path: &Path) -> io::Result<SystemTime> {
    fs::metadata(path)?.modified()
}

const DEFAULT_FILE: &str = r##"# Glide settings. Save this file and Glide applies the changes immediately.

# Base feel: "magic-mouse", "trackpad", "subtle" or "snappy".
preset = "magic-mouse"

# Reverse the scroll direction, like macOS "natural scrolling".
natural = false

# Also smooth free-spinning or high-resolution wheels, which already send fine steps.
smooth_hires = false

# Fine-tuning. Remove the leading "# " from a line to override the preset.
# The values shown are the magic-mouse preset.
[tuning]
# How far one notch scrolls, in notches. 1.0 is Windows' normal (usually 3 lines).
# distance = 1.0

# How long one notch takes to glide to a stop, in milliseconds. Lower is snappier.
# glide_ms = 250

# Keep coasting after a quick flick of the wheel.
# momentum = true

# How fast coasting slows: speed is multiplied by this every millisecond.
# macOS uses 0.998. Lower values (0.995, 0.99) stop sooner.
# deceleration = 0.998

# A flick is this many notches, each less than flick_gap_ms after the previous one.
# flick_notches = 3
# flick_gap_ms = 45

# Extra distance for fast consecutive notches. 0 turns it off; 2.0 means up to 3x.
# acceleration = 2.0
# Notches further apart than this get no extra distance.
# acceleration_window_ms = 120

# Speed limit, in notches per second.
# max_speed = 80
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_parses_to_the_magic_mouse_preset() {
        let config: Config = toml::from_str(DEFAULT_FILE).unwrap();
        assert_eq!(config.params(), Preset::MagicMouse.params());
    }

    #[test]
    fn documented_defaults_match_the_preset() {
        let uncommented = DEFAULT_FILE.replace("# distance", "distance")
            .replace("# glide_ms", "glide_ms")
            .replace("# momentum =", "momentum =")
            .replace("# deceleration =", "deceleration =")
            .replace("# flick_notches", "flick_notches")
            .replace("# flick_gap_ms", "flick_gap_ms")
            .replace("# acceleration =", "acceleration =")
            .replace("# acceleration_window_ms", "acceleration_window_ms")
            .replace("# max_speed", "max_speed");
        let config: Config = toml::from_str(&uncommented).unwrap();
        let (a, b) = (config.params(), Preset::MagicMouse.params());
        assert!((a.ease_friction - b.ease_friction).abs() < 0.02, "{a:?}");
        assert!((a.momentum_friction - b.momentum_friction).abs() < 1e-9);
        assert_eq!(a.step, b.step);
        assert_eq!(a.fling_notches, b.fling_notches);
        assert!((a.fling_interval - b.fling_interval).abs() < 1e-12);
        assert!((a.acceleration_window - b.acceleration_window).abs() < 1e-12);
        assert_eq!(a.max_velocity, b.max_velocity);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<Config>("presett = \"subtle\"").is_err());
    }
}
