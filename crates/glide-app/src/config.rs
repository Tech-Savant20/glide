//! `%APPDATA%\Glide\config.toml`: a preset plus optional fine-tuning, reloaded
//! whenever the file is saved. The settings window writes it too, through
//! [`save`], which renders the whole file with explanatory comments.

use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::{fs, io};

use glide_engine::{friction_from_per_ms, Params, Preset, NOTCH};
use glide_win::Options;
use serde::Deserialize;

/// A single notch settles when 95% of its distance has been covered: e^(-k t) = 1/20.
const SETTLE: f64 = 2.995_732_273_553_991; // ln(20)

#[derive(Clone, Debug, Deserialize, PartialEq)]
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
    pub const ALL: [PresetName; 4] = [
        PresetName::MagicMouse,
        PresetName::Trackpad,
        PresetName::Subtle,
        PresetName::Snappy,
    ];

    pub fn preset(self) -> Preset {
        match self {
            PresetName::MagicMouse => Preset::MagicMouse,
            PresetName::Trackpad => Preset::Trackpad,
            PresetName::Subtle => Preset::Subtle,
            PresetName::Snappy => Preset::Snappy,
        }
    }

    /// The name used in the config file.
    pub fn key(self) -> &'static str {
        match self {
            PresetName::MagicMouse => "magic-mouse",
            PresetName::Trackpad => "trackpad",
            PresetName::Subtle => "subtle",
            PresetName::Snappy => "snappy",
        }
    }
}

/// User-facing knobs. Each one left unset keeps the preset's value.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
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

/// Every knob with a concrete value: the override if set, else the preset's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knobs {
    pub distance: f64,
    pub glide_ms: f64,
    pub momentum: bool,
    pub deceleration: f64,
    pub flick_gap_ms: f64,
    pub flick_notches: u32,
    pub acceleration: f64,
    pub acceleration_window_ms: f64,
    pub max_speed: f64,
}

impl Knobs {
    fn from_params(p: &Params) -> Self {
        Self {
            distance: p.step / NOTCH,
            glide_ms: SETTLE * 1000.0 / p.ease_friction,
            momentum: p.momentum,
            deceleration: (-p.momentum_friction / 1000.0).exp(),
            flick_gap_ms: p.fling_interval * 1000.0,
            flick_notches: p.fling_notches,
            acceleration: p.acceleration,
            acceleration_window_ms: p.acceleration_window * 1000.0,
            max_speed: p.max_velocity / NOTCH,
        }
    }
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

    /// The effective value of every knob.
    pub fn knobs(&self) -> Knobs {
        Knobs::from_params(&self.params())
    }

    /// The preset's value of every knob, ignoring overrides.
    pub fn preset_knobs(&self) -> Knobs {
        Knobs::from_params(&self.preset.preset().params())
    }

    pub fn options(&self) -> Options {
        Options {
            natural: self.natural,
            smooth_hires: self.smooth_hires,
        }
    }

    /// One line describing the effective tuning, printed after every reload.
    pub fn summary(&self) -> String {
        let k = self.knobs();
        format!(
            "{} | distance {:.2} | glide {:.0} ms | momentum {} (deceleration {:.4}, flick {} notches < {:.0} ms) | acceleration {:.2} within {:.0} ms | max {:.0} notches/s{}",
            self.preset.preset().name(),
            k.distance,
            k.glide_ms,
            if k.momentum { "on" } else { "off" },
            k.deceleration,
            k.flick_notches,
            k.flick_gap_ms,
            k.acceleration,
            k.acceleration_window_ms,
            k.max_speed,
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
        save(path, &Config::default()).map_err(|e| e.to_string())?;
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

/// Writes the whole file. It goes to a temporary file first and is then moved
/// into place, so the running Glide never reloads a half-written file.
pub fn save(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("toml.tmp");
    fs::write(&temp, render(config))?;
    fs::rename(&temp, path)
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| quote(s)).collect();
    format!("[{}]", quoted.join(", "))
}

/// A setting line: the value if set, else a commented-out example.
fn line(name: &str, value: Option<String>, example: &str) -> String {
    match value {
        Some(v) => format!("{name} = {v}"),
        None => format!("# {name} = {example}"),
    }
}

/// The config file for `config`, with every setting explained.
pub fn render(config: &Config) -> String {
    let t = &config.tuning;
    let p = config.preset_knobs();
    let f = |v: f64| format!("{v:?}");
    let exam_apps = (!config.exam_apps.is_empty()).then(|| list(&config.exam_apps));
    let excluded = (!config.excluded_apps.is_empty()).then(|| list(&config.excluded_apps));
    let hotkey = config.toggle_hotkey.as_deref().map(quote);
    format!(
        r##"# Glide settings. The settings window writes this file, and Glide applies
# changes as soon as it's saved. You can also edit it by hand.

# Base feel: "magic-mouse", "trackpad", "subtle" or "snappy".
preset = "{preset}"

# Reverse the scroll direction, like macOS "natural scrolling".
natural = {natural}

# Also smooth free-spinning or high-resolution wheels, which already send fine steps.
smooth_hires = {smooth_hires}

# Stop Glide completely (it stops reading your mouse) while exam lockdown or
# proctoring software runs: Safe Exam Browser, Respondus LockDown Browser,
# Examplify, Guardian Browser, Pearson OnVUE and Inspera. Glide resumes by itself
# when the exam app closes.
pause_during_exams = {exams}

# Other programs that should pause Glide the same way, by process name.
{exam_apps}

# Also stop Glide completely while a game with anti-cheat runs (VALORANT and
# League of Legends with Riot Vanguard, and games using Easy Anti-Cheat or
# BattlEye), so the anti-cheat never sees an input tool running.
pause_during_anti_cheat_games = {games}

# Apps that should keep normal, unsmoothed scrolling. Glide already skips games,
# apps with their own smooth scrolling (like the new Notepad and Settings), WPF
# apps, admin windows and fullscreen games; add anything else here.
{excluded}

# A shortcut that turns Glide on and off from anywhere. Off unless set.
{hotkey}

# Fine-tuning. Each setting overrides the preset; a line starting with "# " uses
# the preset's value, which is the value shown.
[tuning]
# How far one notch scrolls, in notches. 1.0 is Windows' normal (usually 3 lines).
{distance}

# How long a scroll takes to glide to a stop, in milliseconds. Lower is snappier.
{glide_ms}

# Keep coasting after a quick flick of the wheel.
{momentum}

# How fast coasting slows: speed is multiplied by this every millisecond.
# macOS uses 0.998. Lower values (0.995, 0.99) stop sooner.
{deceleration}

# A flick is this many notches, each less than flick_gap_ms after the previous one.
{flick_notches}
{flick_gap_ms}

# Extra distance for fast consecutive notches. 0 turns it off; 2.0 means up to 3x.
{acceleration}
# Notches further apart than this get no extra distance.
{acceleration_window_ms}

# Speed limit, in notches per second.
{max_speed}
"##,
        preset = config.preset.key(),
        natural = config.natural,
        smooth_hires = config.smooth_hires,
        exams = config.pause_during_exams,
        exam_apps = line("exam_apps", exam_apps, "[\"MyUniversityExam.exe\"]"),
        games = config.pause_during_anti_cheat_games,
        excluded = line("excluded_apps", excluded, "[\"SomeApp.exe\"]"),
        hotkey = line("toggle_hotkey", hotkey, "\"Ctrl+Alt+G\""),
        distance = line("distance", t.distance.map(f), &format!("{:.2}", p.distance)),
        glide_ms = line("glide_ms", t.glide_ms.map(f), &format!("{:.0}", p.glide_ms)),
        momentum = line(
            "momentum",
            t.momentum.map(|v| v.to_string()),
            &p.momentum.to_string()
        ),
        deceleration = line(
            "deceleration",
            t.deceleration.map(f),
            &format!("{:.4}", p.deceleration)
        ),
        flick_notches = line(
            "flick_notches",
            t.flick_notches.map(|v| v.to_string()),
            &p.flick_notches.to_string()
        ),
        flick_gap_ms = line(
            "flick_gap_ms",
            t.flick_gap_ms.map(f),
            &format!("{:.0}", p.flick_gap_ms)
        ),
        acceleration = line(
            "acceleration",
            t.acceleration.map(f),
            &format!("{:.1}", p.acceleration)
        ),
        acceleration_window_ms = line(
            "acceleration_window_ms",
            t.acceleration_window_ms.map(f),
            &format!("{:.0}", p.acceleration_window_ms)
        ),
        max_speed = line(
            "max_speed",
            t.max_speed.map(f),
            &format!("{:.0}", p.max_speed)
        ),
    )
}

/// Changes just the excluded apps, keeping every other setting.
pub fn write_excluded_apps(path: &Path, apps: &[String]) -> io::Result<()> {
    let mut config = load(path).map_err(io::Error::other)?;
    config.excluded_apps = apps.to_vec();
    save(path, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Config {
        toml::from_str(text).unwrap_or_else(|e| panic!("{e}\n{text}"))
    }

    #[test]
    fn default_file_parses_to_the_magic_mouse_preset() {
        let config = parse(&render(&Config::default()));
        assert_eq!(config, Config::default());
        assert_eq!(config.params(), Preset::MagicMouse.params());
    }

    #[test]
    fn every_setting_round_trips() {
        let config = Config {
            preset: PresetName::Snappy,
            natural: true,
            smooth_hires: true,
            pause_during_exams: false,
            exam_apps: vec!["My Exam.exe".into(), r#"odd"name\.exe"#.into()],
            pause_during_anti_cheat_games: false,
            excluded_apps: vec!["Foo.exe".into()],
            toggle_hotkey: Some("Ctrl+Alt+G".into()),
            tuning: Tuning {
                distance: Some(1.5),
                glide_ms: Some(180.0),
                momentum: Some(false),
                deceleration: Some(0.995),
                flick_gap_ms: Some(30.0),
                flick_notches: Some(4),
                acceleration: Some(0.0),
                acceleration_window_ms: Some(90.0),
                max_speed: Some(25.0),
            },
        };
        assert_eq!(parse(&render(&config)), config);
    }

    #[test]
    fn commented_values_match_the_preset() {
        for preset in PresetName::ALL {
            let config = Config {
                preset,
                ..Config::default()
            };
            let uncommented: String = render(&config)
                .lines()
                .map(|l| match l.strip_prefix("# ") {
                    Some(rest)
                        if rest.contains(" = ") && !rest.contains('[') && !rest.contains('"') =>
                    {
                        rest
                    }
                    _ => l,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let (a, b) = (parse(&uncommented).knobs(), config.knobs());
            assert!((a.glide_ms - b.glide_ms).abs() < 1.0, "{preset:?}");
            assert!((a.deceleration - b.deceleration).abs() < 1e-4, "{preset:?}");
            assert_eq!(a.flick_notches, b.flick_notches);
            assert!((a.max_speed - b.max_speed).abs() < 0.5);
            assert_eq!(a.momentum, b.momentum);
        }
    }

    #[test]
    fn saving_is_atomic_and_readable() {
        let dir = std::env::temp_dir().join(format!("glide-config-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let config = Config {
            excluded_apps: vec!["Foo.exe".into()],
            ..Config::default()
        };
        save(&path, &config).unwrap();
        assert_eq!(load(&path).unwrap(), config);
        write_excluded_apps(&path, &["Bar.exe".into()]).unwrap();
        assert_eq!(
            load(&path).unwrap().excluded_apps,
            vec!["Bar.exe".to_string()]
        );
        assert!(!path.with_extension("toml.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn exam_pause_is_on_unless_turned_off() {
        assert!(Config::default().pause_during_exams);
        assert!(parse("").pause_during_exams);
        assert!(!parse("pause_during_exams = false").pause_during_exams);
        assert!(Config::default().pause_during_anti_cheat_games);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<Config>("presett = \"subtle\"").is_err());
    }
}
