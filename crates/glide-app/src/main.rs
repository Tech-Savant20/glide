//! Prototype: smooths the mouse wheel until Enter is pressed, reloading
//! `%APPDATA%\Glide\config.toml` whenever it is saved and pausing while exam
//! software runs.

mod config;
mod exam;

use std::thread;
use std::time::{Duration, Instant};

use glide_win::Smoother;

/// How often to look for exam software.
const EXAM_CHECK: Duration = Duration::from_secs(2);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = config::path();
    let config = config::load_or_create(&path).map_err(|e| format!("{}: {e}", path.display()))?;

    let smoother = Smoother::start(config.params(), config.options())?;
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
            let mut last_exam_check = Instant::now() - EXAM_CHECK;
            loop {
                let now = config::modified(path).ok();
                if now != seen {
                    seen = now;
                    match config::load(path) {
                        Ok(fresh) => {
                            smoother.set_params(fresh.params());
                            smoother.set_options(fresh.options());
                            println!("Reloaded: {}", fresh.summary());
                            config = fresh;
                            last_exam_check = Instant::now() - EXAM_CHECK;
                        }
                        Err(e) => println!("Couldn't read settings, keeping the old ones: {e}"),
                    }
                }

                if last_exam_check.elapsed() >= EXAM_CHECK {
                    last_exam_check = Instant::now();
                    let exam = if config.pause_during_exams {
                        glide_win::running_process_names()
                            .ok()
                            .and_then(|running| {
                                exam::find_running(&running, &config.exam_apps).map(String::from)
                            })
                    } else {
                        None
                    };
                    match (&paused_for, exam) {
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

                thread::sleep(Duration::from_millis(300));
            }
        });

        let _ = std::io::stdin().read_line(&mut String::new());
        std::process::exit(0);
    })
}
