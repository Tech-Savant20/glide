//! Prototype: smooths the mouse wheel until Enter is pressed, reloading
//! `%APPDATA%\Glide\config.toml` whenever it is saved.

mod config;

use std::thread;
use std::time::Duration;

use glide_win::Smoother;

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
            let mut seen = config::modified(path).ok();
            loop {
                thread::sleep(Duration::from_millis(300));
                let now = config::modified(path).ok();
                if now == seen {
                    continue;
                }
                seen = now;
                match config::load(path) {
                    Ok(config) => {
                        smoother.set_params(config.params());
                        smoother.set_options(config.options());
                        println!("Reloaded: {}", config.summary());
                    }
                    Err(e) => println!("Couldn't read settings, keeping the old ones: {e}"),
                }
            }
        });

        let _ = std::io::stdin().read_line(&mut String::new());
        std::process::exit(0);
    })
}
