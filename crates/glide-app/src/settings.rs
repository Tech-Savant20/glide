//! The settings window, `glide --settings`.
//!
//! It runs as its own process so the always-running tray part of Glide stays
//! small, and it only talks to that part through the settings file: every
//! change is saved (after a short pause while you drag a slider) and the running
//! Glide reloads it within a moment. That also makes the "Try it here" list a
//! true preview, because the running Glide is what smooths it.

slint::include_modules!();

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use glide_engine::{Axis, Engine, Params};
use slint::{
    ComponentHandle, ModelRc, SharedString, StandardListViewItem, Timer, TimerMode, VecModel,
};
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOWNORMAL,
};

use crate::config::{self, Config, PresetName, Tuning};
use crate::{apps, autostart, hotkey, log};

/// Settings are saved this long after the last change, so dragging a slider
/// doesn't rewrite the file dozens of times a second.
const SAVE_DELAY: Duration = Duration::from_millis(250);

/// Coasting is shown as a time constant; the file stores Apple-style
/// per-millisecond deceleration. `d = e^(-1/τ)`.
fn coast_ms(deceleration: f64) -> f64 {
    -1.0 / deceleration.ln()
}

fn deceleration(coast_ms: f64) -> f64 {
    (-1.0 / coast_ms.max(1.0)).exp()
}

struct Editor {
    config: Config,
    /// Apps with a window open right now, offered in "Add an open app".
    running: Vec<String>,
}

pub fn run() -> Result<(), slint::PlatformError> {
    // One settings window at a time: bring the open one to the front instead.
    let _instance = unsafe { CreateMutexW(None, true, w!("Local\\Glide.Settings")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            if let Ok(hwnd) = FindWindowW(None, w!("Glide Settings")) {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
        }
        return Ok(());
    }

    let path = config::path();
    let (config, problem) = match config::load_or_create(&path) {
        Ok(config) => (config, None),
        Err(e) => (
            Config::default(),
            Some(format!(
                "Your settings file has a mistake ({e}). Changing anything here replaces it."
            )),
        ),
    };

    let ui = SettingsWindow::new()?;
    let editor = Rc::new(RefCell::new(Editor {
        config,
        running: Vec::new(),
    }));
    let save_timer = Rc::new(Timer::default());

    let names: Vec<SharedString> = PresetName::ALL
        .iter()
        .map(|p| p.preset().name().into())
        .collect();
    ui.set_presets(ModelRc::new(VecModel::from(names)));
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.set_autostart(autostart::is_enabled());
    ui.set_skipped_summary(
        "Glide always leaves admin windows, fullscreen games and WPF apps alone, plus a built-in \
         list of games and Windows apps that already scroll smoothly (such as Notepad, Settings \
         and Photos). Add anything else here."
            .into(),
    );
    refresh_running(&editor);
    show(&ui, &editor.borrow());
    let hotkey_text = editor
        .borrow()
        .config
        .toggle_hotkey
        .clone()
        .unwrap_or_default();
    ui.set_hotkey(hotkey_text.clone().into());
    ui.set_hotkey_status(hotkey_status(&hotkey_text).1.into());
    ui.set_status(problem.unwrap_or_default().into());

    // Applies `change` to the settings, refreshes the window and saves soon.
    let edit = {
        let ui = ui.as_weak();
        let editor = editor.clone();
        let save_timer = save_timer.clone();
        let path = path.clone();
        move |change: &dyn Fn(&mut Config)| {
            change(&mut editor.borrow_mut().config);
            let Some(ui) = ui.upgrade() else { return };
            show(&ui, &editor.borrow());
            ui.set_status("".into());
            let (weak, editor, path) = (ui.as_weak(), editor.clone(), path.clone());
            save_timer.start(TimerMode::SingleShot, SAVE_DELAY, move || {
                let result = config::save(&path, &editor.borrow().config);
                if let Some(ui) = weak.upgrade() {
                    ui.set_status(match &result {
                        Ok(()) => "Saved.".into(),
                        Err(e) => format!("Couldn't save: {e}").into(),
                    });
                }
                if let Err(e) = result {
                    log::write(&format!("Settings window couldn't save: {e}"));
                }
            });
        }
    };
    let edit = Rc::new(edit);

    let e = edit.clone();
    ui.on_preset_selected(move |i| {
        e(&|c: &mut Config| {
            c.preset = PresetName::ALL[(i.max(0) as usize).min(PresetName::ALL.len() - 1)];
            // A new feel starts from that preset's own fine-tuning.
            c.tuning = Tuning::default();
        })
    });
    let e = edit.clone();
    ui.on_natural_changed(move |on| e(&|c: &mut Config| c.natural = on));
    let e = edit.clone();
    ui.on_momentum_changed(move |on| e(&|c: &mut Config| c.tuning.momentum = Some(on)));
    let e = edit.clone();
    ui.on_knob_changed(move |name, v| {
        let v = v as f64;
        e(&|c: &mut Config| {
            let t = &mut c.tuning;
            match name.as_str() {
                "distance" => t.distance = Some((v * 20.0).round() / 20.0),
                "glide_ms" => t.glide_ms = Some(v.round()),
                "coast_ms" => t.deceleration = Some(deceleration((v / 10.0).round() * 10.0)),
                "acceleration" => t.acceleration = Some((v * 10.0).round() / 10.0),
                "max_speed" => t.max_speed = Some(v.round()),
                _ => {}
            }
        })
    });
    let e = edit.clone();
    ui.on_reset_tuning(move || e(&|c: &mut Config| c.tuning = Tuning::default()));

    let e = edit.clone();
    let ui_weak = ui.as_weak();
    ui.on_remove_excluded(move |i| {
        e(&|c: &mut Config| {
            if (0..c.excluded_apps.len() as i32).contains(&i) {
                c.excluded_apps.remove(i as usize);
            }
        });
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_excluded_current(-1);
        }
    });
    let e = edit.clone();
    let ed = editor.clone();
    let ui_weak = ui.as_weak();
    ui.on_add_running(move |i| {
        let Some(app) = ed.borrow().running.get(i.max(0) as usize).cloned() else {
            return;
        };
        e(&|c: &mut Config| {
            if !c.excluded_apps.iter().any(|x| x.eq_ignore_ascii_case(&app)) {
                c.excluded_apps.push(app.clone());
            }
        });
        // The app now shows in the list above, so stop offering it below.
        refresh_running(&ed);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_running_current(-1);
            show(&ui, &ed.borrow());
        }
    });
    let ed = editor.clone();
    let ui_weak = ui.as_weak();
    ui.on_refresh_running(move || {
        refresh_running(&ed);
        if let Some(ui) = ui_weak.upgrade() {
            show(&ui, &ed.borrow());
        }
    });

    let e = edit.clone();
    ui.on_smooth_hires_changed(move |on| e(&|c: &mut Config| c.smooth_hires = on));
    let e = edit.clone();
    ui.on_exams_changed(move |on| e(&|c: &mut Config| c.pause_during_exams = on));
    let e = edit.clone();
    ui.on_games_changed(move |on| e(&|c: &mut Config| c.pause_during_anti_cheat_games = on));

    let e = edit.clone();
    let ui_weak = ui.as_weak();
    ui.on_hotkey_changed(move |text| {
        let (value, message) = hotkey_status(&text);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_hotkey_status(message.into());
        }
        if let Some(value) = value {
            e(&|c: &mut Config| c.toggle_hotkey = value.clone());
        }
    });

    let ui_weak = ui.as_weak();
    ui.on_autostart_changed(move |on| {
        let result = autostart::set_enabled(on);
        if let Some(ui) = ui_weak.upgrade() {
            match result {
                Ok(()) => ui.set_status(
                    if on {
                        "Glide will start when you sign in."
                    } else {
                        "Glide won't start by itself."
                    }
                    .into(),
                ),
                Err(e) => {
                    ui.set_autostart(autostart::is_enabled());
                    ui.set_status(format!("Couldn't change that: {e}").into());
                }
            }
        }
    });

    let config_path = path.clone();
    ui.on_open_config(move || open_in_notepad(&config_path));
    ui.on_open_log(|| open_in_notepad(&log::path()));
    ui.on_open_slint(|| open_url("https://slint.dev"));

    // `--tab apps` or `--tab general` opens on that tab.
    let args: Vec<String> = std::env::args().collect();
    if let Some(name) = args
        .iter()
        .position(|a| a == "--tab")
        .and_then(|i| args.get(i + 1))
    {
        ui.set_tab(match name.as_str() {
            "apps" => 1,
            "general" => 2,
            _ => 0,
        });
    }

    ui.run()
}

/// What a typed shortcut means: `Some(new setting)` if it can be saved, and a
/// message to show under the box.
fn hotkey_status(text: &str) -> (Option<Option<String>>, String) {
    let text = text.trim();
    if text.is_empty() {
        return (Some(None), "No shortcut. Type one like Ctrl+Alt+G.".into());
    }
    match hotkey::parse(text) {
        Ok(_) => (
            Some(Some(text.to_string())),
            format!("Press {text} anywhere to turn Glide on or off."),
        ),
        Err(e) => (None, format!("Not saved yet: {e}.")),
    }
}

fn refresh_running(editor: &Rc<RefCell<Editor>>) {
    let mut editor = editor.borrow_mut();
    let excluded = editor.config.excluded_apps.clone();
    editor.running = glide_win::visible_window_apps()
        .into_iter()
        .filter(|app| !apps::is_built_in(app))
        .filter(|app| !excluded.iter().any(|x| x.eq_ignore_ascii_case(app)))
        .collect();
}

/// Copies the settings into the window.
fn show(ui: &SettingsWindow, editor: &Editor) {
    let config = &editor.config;
    let knobs = config.knobs();
    let index = PresetName::ALL
        .iter()
        .position(|p| *p == config.preset)
        .unwrap_or(0);
    ui.set_preset_index(index as i32);
    ui.set_natural(config.natural);
    ui.set_momentum(knobs.momentum);
    ui.set_distance(knobs.distance as f32);
    ui.set_glide_ms(knobs.glide_ms as f32);
    ui.set_coast_ms(coast_ms(knobs.deceleration) as f32);
    ui.set_acceleration(knobs.acceleration as f32);
    ui.set_max_speed(knobs.max_speed as f32);
    ui.set_tuned(config.tuning != Tuning::default());

    let (notch, roll) = curves(&config.params());
    ui.set_curve_notch(notch.into());
    ui.set_curve_roll(roll.into());

    let excluded: Vec<StandardListViewItem> = config
        .excluded_apps
        .iter()
        .map(|a| StandardListViewItem::from(SharedString::from(a.as_str())))
        .collect();
    ui.set_excluded(ModelRc::new(VecModel::from(excluded)));
    let running: Vec<SharedString> = editor.running.iter().map(|a| a.as_str().into()).collect();
    let has_running = !running.is_empty();
    ui.set_running_apps(ModelRc::new(VecModel::from(running)));
    if has_running && ui.get_running_current() < 0 {
        ui.set_running_current(0);
    } else if !has_running {
        ui.set_running_current(-1);
    }

    ui.set_smooth_hires(config.smooth_hires);
    ui.set_exams(config.pause_during_exams);
    ui.set_games(config.pause_during_anti_cheat_games);
}

/// Speed-over-time paths (in a 1000 × 100 viewbox) for one notch and for five
/// notches rolled 150 ms apart, simulated with the real engine.
fn curves(params: &Params) -> (String, String) {
    const SPAN: f64 = 1.2;
    const DT: f64 = 1.0 / 240.0;
    let sample = |notches: &[f64]| {
        let mut engine = Engine::new(params.clone());
        let mut speeds = Vec::new();
        let (mut t, mut next) = (0.0, 0);
        while t < SPAN {
            while next < notches.len() && notches[next] <= t {
                engine.on_notch(Axis::Vertical, -1.0, notches[next]);
                next += 1;
            }
            engine.tick(DT);
            speeds.push(engine.velocity(Axis::Vertical).abs());
            t += DT;
        }
        speeds
    };
    let notch = sample(&[0.0]);
    let roll = sample(&[0.0, 0.15, 0.3, 0.45, 0.6]);
    let top = notch.iter().chain(&roll).fold(1.0f64, |m, &v| m.max(v));
    let path = |speeds: &[f64]| {
        let n = speeds.len().max(2) as f64 - 1.0;
        let mut d = String::from("M 0 100");
        for (i, v) in speeds.iter().enumerate() {
            d.push_str(&format!(
                " L {:.1} {:.1}",
                i as f64 / n * 1000.0,
                100.0 - v / top * 95.0
            ));
        }
        d
    };
    (path(&notch), path(&roll))
}

fn open_in_notepad(path: &std::path::Path) {
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("notepad.exe"),
            &HSTRING::from(path.as_os_str()),
            None,
            SW_SHOWNORMAL,
        );
    }
}

fn open_url(url: &str) {
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(url),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coasting_time_round_trips_to_apple_deceleration() {
        let tau = coast_ms(0.998);
        assert!((tau - 499.5).abs() < 0.1, "{tau}");
        assert!((deceleration(tau) - 0.998).abs() < 1e-12);
    }

    #[test]
    fn curves_start_at_zero_and_stay_in_the_viewbox() {
        let (notch, roll) = curves(&glide_engine::Preset::MagicMouse.params());
        let heights = |d: &str| -> Vec<f64> {
            assert!(d.starts_with("M 0 100 L"));
            d.split(" L ")
                .skip(1)
                .map(|p| p.split(' ').nth(1).unwrap().parse().unwrap())
                .collect()
        };
        let (notch, roll) = (heights(&notch), heights(&roll));
        for ys in [&notch, &roll] {
            assert!(ys.iter().all(|y| (0.0..=100.0).contains(y)));
        }
        let top = |ys: &[f64]| ys.iter().cloned().fold(100.0, f64::min);
        // The roll is the faster motion, so it sets the scale.
        assert!(top(&roll) < 6.0);
        assert!(top(&notch) < 80.0 && top(&notch) > top(&roll));
    }

    #[test]
    fn hotkey_status_only_saves_valid_shortcuts() {
        assert_eq!(hotkey_status("").0, Some(None));
        assert_eq!(
            hotkey_status(" Ctrl+Alt+G ").0,
            Some(Some("Ctrl+Alt+G".into()))
        );
        assert_eq!(hotkey_status("G").0, None);
    }
}
