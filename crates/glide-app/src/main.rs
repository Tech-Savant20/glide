//! M0 prototype: smooths the mouse wheel until Enter is pressed.
//!
//! Usage: `glide [magic-mouse|trackpad|subtle|snappy] [--natural]`

use glide_engine::Preset;
use glide_win::{Options, Smoother};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut preset = Preset::MagicMouse;
    let mut options = Options::default();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "magic-mouse" => preset = Preset::MagicMouse,
            "trackpad" => preset = Preset::Trackpad,
            "subtle" => preset = Preset::Subtle,
            "snappy" => preset = Preset::Snappy,
            "--natural" => options.natural = true,
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let smoother = Smoother::start(preset.params(), options)?;
    println!(
        "Glide is smoothing your mouse wheel ({}). Press Enter to quit.",
        preset.name()
    );
    let _ = std::io::stdin().read_line(&mut String::new());
    smoother.shutdown();
    Ok(())
}
