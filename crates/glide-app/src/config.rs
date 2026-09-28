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

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub preset: PresetName,
    pub natural: bool,
    pub smooth_hires: bool,
    /// Remove the mouse hook while exam lockdown or proctoring software runs.
    pub pause_during_exams: bool,
    /// Extra process names that count as exam software.
    pub exam_apps: Vec<String>,
    /// Remove the mouse hook while a game with anti-cheat runs.
    pub pause_during_anti_cheat_games: bool,
    /// Extra apps (like `game.exe`) that always get raw wheel events.
    pub excluded_apps: Vec<String>,
    /// Optional global shortcut, like `Ctrl+Alt+G`, that turns Glide on and off.
    pub toggle_hotkey: Option<String>,
    pub tuning: Tuning,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            preset: PresetName::default(),
            natural: false,
            smooth_hires: false,
            pause_during_exams: true,
            exam_apps: Vec::new(),
            pause_during_anti_cheat_games: true,
            excluded_apps: Vec::new(),
            toggle_hotkey: None,
            tuning: Tuning::default(),
        }
    }
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

/// Rewrites just the `excluded_apps` setting, keeping the rest of the file
/// (comments included) as the user left it.
pub fn write_excluded_apps(path: &Path, apps: &[String]) -> io::Result<()> {
    let text = fs::read_to_string(path)?;
    fs::write(path, with_excluded_apps(&text, apps))
}

fn with_excluded_apps(text: &str, apps: &[String]) -> String {
    let quoted: Vec<String> = apps
        .iter()
        .map(|a| format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    let setting = format!("excluded_apps = [{}]", quoted.join(", "));
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let starts = |prefix: &str| lines.iter().position(|l| l.trim_start().starts_with(prefix));
    if let Some(i) = starts("excluded_apps") {
        lines[i] = setting;
    } else if let Some(i) = starts("# excluded_apps") {
        lines.insert(i + 1, setting);
    } else if let Some(i) = lines.iter().position(|l| l.trim() == "[tuning]") {
        // Top-level settings must come before the first table.
        lines.insert(i, String::new());
        lines.insert(i, setting);
    } else {
        lines.insert(0, setting);
    }
    lines.join("\n") + "\n"
}

const DEFAULT_FILE: &str = r##"# Glide settings. Save this file and Glide applies the changes immediately.

# Base feel: "magic-mouse", "trackpad", "subtle" or "snappy".
preset = "magic-mouse"

# Reverse the scroll direction, like macOS "natural scrolling".
natural = false

# Also smooth free-spinning or high-resolution wheels, which already send fine steps.
smooth_hires = false

# Stop Glide completely (it stops reading your mouse) while exam lockdown or
# proctoring software runs: Safe Exam Browser, Respondus LockDown Browser,
# Examplify, Guardian Browser, Pearson OnVUE and Inspera. Glide resumes by itself
# when the exam app closes.
pause_during_exams = true

# Other programs that should pause Glide the same way, by process name.
# exam_apps = ["MyUniversityExam.exe"]

# Also stop Glide completely while a game with anti-cheat runs (VALORANT and
# League of Legends with Riot Vanguard, and games using Easy Anti-Cheat or
# BattlEye), so the anti-cheat never sees an input tool running.
pause_during_anti_cheat_games = true

# Apps that should keep normal, unsmoothed scrolling. Glide already skips games,
# apps with their own smooth scrolling (like the new Notepad and Settings), WPF
# apps, admin windows and fullscreen games; add anything else here.
# excluded_apps = ["SomeApp.exe"]

# A shortcut that turns Glide on and off from anywhere. Off unless set.
# toggle_hotkey = "Ctrl+Alt+G"

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
# max_speed = 40
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
    fn exam_pause_is_on_unless_turned_off() {
        assert!(Config::default().pause_during_exams);
        assert!(toml::from_str::<Config>("").unwrap().pause_during_exams);
        let off: Config = toml::from_str("pause_during_exams = false").unwrap();
        assert!(!off.pause_during_exams);
        assert!(Config::default().pause_during_anti_cheat_games);
    }

    #[test]
    fn excluded_apps_can_be_rewritten_without_losing_the_rest() {
        let apps = vec!["Foo.exe".to_string(), "Bar Baz.exe".to_string()];
        let once = with_excluded_apps(DEFAULT_FILE, &apps);
        let config: Config = toml::from_str(&once).unwrap();
        assert_eq!(config.excluded_apps, apps);
        assert!(once.contains("# glide_ms = 250"), "comments kept");

        let twice = with_excluded_apps(&once, &apps[..1]);
        let config: Config = toml::from_str(&twice).unwrap();
        assert_eq!(config.excluded_apps, &apps[..1]);
        let settings = twice.lines().filter(|l| l.starts_with("excluded_apps")).count();
        assert_eq!(settings, 1, "{twice}");

        let bare = with_excluded_apps("[tuning]\nglide_ms = 200\n", &apps);
        let config: Config = toml::from_str(&bare).unwrap();
        assert_eq!(config.excluded_apps, apps);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<Config>("presett = \"subtle\"").is_err());
    }
}
