//! Prototype: smooths the mouse wheel until Enter is pressed, reloading
//! `%APPDATA%\Glide\config.toml` whenever it is saved and pausing while exam
//! software or anti-cheat games run.

mod apps;
mod config;
mod pause;

use std::thread;
use std::time::{Duration, Instant};

use glide_win::{Skip, Smoother};

/// How often to look for exam software and anti-cheat games.
const PAUSE_CHECK: Duration = Duration::from_secs(2);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = config::path();
    let config = config::load_or_create(&path).map_err(|e| format!("{}: {e}", path.display()))?;

    let smoother = Smoother::start(config.params(), config.options())?;
    smoother.set_excluded_apps(&apps::excluded(&config.excluded_apps));
    println!("Glide is smoothing your mouse wheel. Press Enter to quit.");
    println!("Settings: {}", path.display());
    println!("  {}", config.summary());

    thread::scope(|scope| {
        let smoother = &smoother;
        let path = &path;
        scope.spawn(move || {
            let mut config = config;
            let mut seen = config::modified(path).ok();
            let mut paused_for: Option<String> = None;
            let mut last_pause_check = Instant::now() - PAUSE_CHECK;
            let mut last_skip = None;
            loop {
                let now = config::modified(path).ok();
                if now != seen {
                    seen = now;
                    match config::load(path) {
                        Ok(fresh) => {
                            smoother.set_params(fresh.params());
                            smoother.set_options(fresh.options());
                            smoother.set_excluded_apps(&apps::excluded(&fresh.excluded_apps));
                            println!("Reloaded: {}", fresh.summary());
                            config = fresh;
                            last_pause_check = Instant::now() - PAUSE_CHECK;
                        }
                        Err(e) => println!("Couldn't read settings, keeping the old ones: {e}"),
                    }
                }

                if last_pause_check.elapsed() >= PAUSE_CHECK {
                    last_pause_check = Instant::now();
                    let blocker = if config.pause_during_exams || config.pause_during_anti_cheat_games {
                        glide_win::running_process_names().ok().and_then(|running| {
                            let exam = config
                                .pause_during_exams
                                .then(|| pause::find_running(&running, pause::EXAM_APPS, &config.exam_apps))
                                .flatten();
                            let game = config
                                .pause_during_anti_cheat_games
                                .then(|| pause::find_running(&running, pause::ANTI_CHEAT_GAMES, &[]))
                                .flatten();
                            exam.or(game).map(String::from)
                        })
                    } else {
                        None
                    };
                    match (&paused_for, blocker) {
                        (None, Some(app)) => {
                            smoother.suspend();
                            println!("Paused: {app} is running. Glide has stopped reading your mouse.");
                            paused_for = Some(app);
                        }
                        (Some(app), None) => {
                            match smoother.resume() {
                                Ok(()) => println!("Resumed: {app} has closed."),
                                Err(e) => println!("Couldn't resume after {app} closed: {e}"),
                            }
                            paused_for = None;
                        }
                        _ => {}
                    }
                }

                // Say when the window under the cursor switches between smoothed and
                // raw, so it's clear why an app scrolls the way it does.
                if !smoother.is_suspended() {
                    let skip = smoother.skip_reason_under_cursor();
                    if skip != last_skip {
                        match skip {
                            None => println!("Here: smoothing."),
                            Some(Skip::Excluded) => println!("Here: normal scrolling (app is on the exclusion list)."),
                            Some(Skip::Elevated) => println!("Here: normal scrolling (admin window; Windows blocks Glide there)."),
                            Some(Skip::FullscreenGame) => println!("Here: normal scrolling (fullscreen game)."),
                            Some(Skip::Wpf) => println!("Here: normal scrolling (WPF app; it would over-scroll)."),
                        }
                        last_skip = skip;
                    }
                }

                thread::sleep(Duration::from_millis(300));
            }
        });

        let _ = std::io::stdin().read_line(&mut String::new());
        std::process::exit(0);
    })
}
