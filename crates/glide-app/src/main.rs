//! Glide: Mac-style smooth scrolling for Windows.
//!
//! Runs in the background with a tray icon. The settings window is a separate
//! program, `glide-settings.exe`. `glide --debug`, run from a terminal, also
//! prints what Glide is doing there.

#![windows_subsystem = "windows"]

mod icon;
mod tray;

use std::sync::Mutex;
use std::time::Instant;

use glide_app::{apps, config, log};
use glide_win::Smoother;
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

/// Everything the tray thread and the watcher thread share.
pub struct Shared {
    pub smoother: Smoother,
    pub state: Mutex<State>,
    pub config_path: std::path::PathBuf,
    pub debug: bool,
}

pub struct State {
    pub config: config::Config,
    /// The user's on/off switch (tray click or hotkey).
    pub enabled: bool,
    /// "Pause for 1 hour" ends at this time.
    pub paused_until: Option<Instant>,
    /// An exam app or anti-cheat game that is running, which pauses Glide.
    pub blocked_by: Option<String>,
    /// The app the user was last working in, for "Normal scrolling in …".
    pub last_app: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    On,
    Off,
    PausedMinutes(u64),
    Blocked(String),
}

impl State {
    pub fn status(&self) -> Status {
        if let Some(app) = &self.blocked_by {
            return Status::Blocked(app.clone());
        }
        if !self.enabled {
            return Status::Off;
        }
        match self.paused_until {
            Some(until) => {
                let left = until.saturating_duration_since(Instant::now()).as_secs();
                Status::PausedMinutes(left.div_ceil(60).max(1))
            }
            None => Status::On,
        }
    }
}

fn fatal(message: &str) -> ! {
    log!("Can't start: {message}");
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            w!("Glide"),
            MB_OK | MB_ICONERROR,
        );
    }
    std::process::exit(1);
}

fn main() {
    let debug = std::env::args().any(|a| a == "--debug");
    if debug {
        // Show output in the terminal that started us.
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
    log::init();

    // `glide --settings` still works: it opens the settings program.
    if std::env::args().any(|a| a == "--settings") {
        let args = std::env::args().skip_while(|a| a != "--settings").skip(1);
        let spawned = glide_app::settings_exe()
            .and_then(|exe| std::process::Command::new(exe).args(args).spawn());
        if let Err(e) = spawned {
            fatal(&format!("couldn't open the settings window: {e}"));
        }
        return;
    }

    // One Glide at a time: two would smooth every scroll twice.
    let _instance = unsafe { CreateMutexW(None, true, w!("Local\\Glide.SingleInstance")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        log!("Glide is already running.");
        return;
    }

    let config_path = config::path();
    let config = config::load_or_create(&config_path)
        .unwrap_or_else(|e| fatal(&format!("couldn't read {}:\n{e}", config_path.display())));

    let smoother = Smoother::start(config.params(), config.options())
        .unwrap_or_else(|e| fatal(&format!("couldn't install the mouse hook: {e}")));
    smoother.set_excluded_apps(&apps::excluded(&config.excluded_apps));
    log!("Started. {}", config.summary());

    let shared = std::sync::Arc::new(Shared {
        smoother,
        state: Mutex::new(State {
            config,
            enabled: true,
            paused_until: None,
            blocked_by: None,
            last_app: None,
        }),
        config_path,
        debug,
    });
    if let Err(e) = tray::run(shared.clone()) {
        fatal(&format!("couldn't create the tray icon: {e}"));
    }
    shared.smoother.suspend();
    log!("Quit.");
    std::process::exit(0);
}
