//! What Glide's two programs share: `glide.exe` (the tray app that smooths
//! scrolling) and `glide-settings.exe` (the settings window). They are separate
//! so the always-running one doesn't carry the settings window's UI toolkit.

pub mod apps;
pub mod autostart;
pub mod config;
pub mod hotkey;
pub mod icon_art;
pub mod log;
pub mod pause;

/// The settings window's executable, next to the running one.
pub fn settings_exe() -> std::io::Result<std::path::PathBuf> {
    Ok(std::env::current_exe()?.with_file_name("glide-settings.exe"))
}
