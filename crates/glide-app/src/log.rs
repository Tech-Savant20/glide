//! A tiny log: `%LOCALAPPDATA%\Glide\glide.log`, plus the console when Glide was
//! started with `--debug` from a terminal. Nothing is ever sent anywhere.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// The log restarts once it grows past this.
const MAX_BYTES: u64 = 1024 * 1024;

static FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

pub fn path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(|| PathBuf::from("."), PathBuf::from);
    base.join("Glide").join("glide.log")
}

pub fn init() {
    let path = path();
    let file = (|| {
        fs::create_dir_all(path.parent()?).ok()?;
        let too_big = fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES);
        OpenOptions::new()
            .create(true)
            .append(!too_big)
            .write(true)
            .truncate(too_big)
            .open(&path)
            .ok()
    })();
    let _ = FILE.set(Mutex::new(file));
}

pub fn write(message: &str) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // UTC time of day is enough to line entries up; the date is in the file's
    // modified time.
    let line = format!(
        "{:02}:{:02}:{:02}Z {message}",
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60
    );
    println!("{line}");
    if let Some(file) = FILE.get() {
        if let Some(file) = file.lock().unwrap_or_else(PoisonError::into_inner).as_mut() {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// Writes a formatted line to the log, like `println!`.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}
