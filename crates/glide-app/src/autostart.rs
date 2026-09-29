//! "Start with Windows", through the per-user Run key. No admin rights needed.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: PCWSTR = w!("Glide");

/// Always the tray program, even when the settings window changes the switch.
fn command() -> Option<String> {
    let exe = std::env::current_exe().ok()?.with_file_name("glide.exe");
    Some(format!("\"{}\"", exe.display()))
}

/// Whether Windows starts this copy of Glide at sign-in.
pub fn is_enabled() -> bool {
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let found = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    }
    .is_ok();
    if !found {
        return false;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let registered = String::from_utf16_lossy(&buf[..len]);
    command().is_some_and(|ours| registered.eq_ignore_ascii_case(&ours))
}

pub fn set_enabled(enabled: bool) -> windows::core::Result<()> {
    unsafe {
        if enabled {
            let command = HSTRING::from(command().unwrap_or_default());
            let bytes = (command.len() + 1) * 2;
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                bytes as u32,
            )
            .ok()
        } else {
            let result = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE);
            // Already absent is fine.
            if result.0 == 2 {
                Ok(())
            } else {
                result.ok()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real registry, so it's opt-in:
    /// `cargo test -p glide-app autostart -- --ignored`
    #[test]
    #[ignore]
    fn autostart_round_trips_and_restores() {
        let before = is_enabled();
        set_enabled(true).unwrap();
        assert!(is_enabled());
        set_enabled(false).unwrap();
        assert!(!is_enabled());
        set_enabled(false).unwrap(); // removing twice is fine
        set_enabled(before).unwrap();
    }
}
