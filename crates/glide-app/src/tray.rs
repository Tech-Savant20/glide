//! The tray icon and its menu, plus the background watcher that reloads settings
//! and notices exam apps and anti-cheat games.
//!
//! The tray lives on the main thread in a hidden window. The watcher runs on its
//! own thread, updates the shared state and asks the tray thread (by posting
//! `WM_REFRESH`) to act on it, so turning the hook on and off and touching the
//! icon only ever happen on the main thread.

use std::cell::{OnceCell, RefCell};
use std::sync::{Arc, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use glide_win::Skip;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICONDATAW_0,
    NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetMessageW, GetSystemMetrics, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON,
    MENU_ITEM_FLAGS, MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, SM_CXSMICON,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP, WM_CONTEXTMENU,
    WM_DESTROY, WM_HOTKEY, WM_NULL, WM_POWERBROADCAST, WNDCLASSW, WS_OVERLAPPED,
};

use crate::icon::{self, Look};
use crate::log::log;
use crate::{apps, autostart, config, hotkey, pause, Shared, State, Status};

const WM_TRAY: u32 = WM_APP + 1;
const WM_REFRESH: u32 = WM_APP + 2;

const ID_TOGGLE: u32 = 1;
const ID_PAUSE: u32 = 2;
const ID_EXCLUDE: u32 = 3;
const ID_SETTINGS: u32 = 4;
const ID_AUTOSTART: u32 = 5;
const ID_QUIT: u32 = 6;

const HOTKEY_ID: i32 = 1;
/// `NIN_SELECT | NINF_KEY`: the icon was activated with the keyboard.
const NIN_KEYSELECT: u32 = NIN_SELECT | 0x1;
const PAUSE_FOR: Duration = Duration::from_secs(60 * 60);
/// How often to look for exam apps and anti-cheat games.
const PAUSE_CHECK: Duration = Duration::from_secs(2);
/// How often to check that Windows hasn't dropped the mouse hook.
const HOOK_CHECK: Duration = Duration::from_secs(3);
/// `PBT_APMRESUMEAUTOMATIC`: the PC woke from sleep or hibernation.
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;

struct Ctx {
    shared: Arc<Shared>,
    icon_on: HICON,
    icon_off: HICON,
    /// Explorer broadcasts this after it restarts; the icon must be added again.
    taskbar_created: u32,
    hotkey: RefCell<Option<String>>,
    /// What was last logged, so the log gets one line per change.
    logged: RefCell<Option<String>>,
}

thread_local! {
    static CTX: OnceCell<Ctx> = const { OnceCell::new() };
}

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared.state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Creates the tray icon and runs the message loop until the user quits.
pub fn run(shared: Arc<Shared>) -> windows::core::Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = w!("GlideTray");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        });
        // A real (hidden) top-level window rather than a message-only one, so it
        // receives Explorer's "TaskbarCreated" broadcast.
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Glide"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        let size = GetSystemMetrics(SM_CXSMICON).max(16) as u32;
        let ctx = Ctx {
            shared: shared.clone(),
            icon_on: icon::create(size, Look::On)?,
            icon_off: icon::create(size, Look::Off)?,
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            hotkey: RefCell::new(None),
            logged: RefCell::new(None),
        };
        CTX.with(|c| {
            let _ = c.set(ctx);
        });

        add_icon(hwnd);
        refresh(hwnd);
        spawn_watcher(shared, hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

fn with_ctx<R>(f: impl FnOnce(&Ctx) -> R) -> Option<R> {
    CTX.with(|c| c.get().map(f))
}

fn notify_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    }
}

fn add_icon(hwnd: HWND) {
    let mut data = notify_data(hwnd);
    data.uFlags = NIF_MESSAGE;
    data.uCallbackMessage = WM_TRAY;
    data.Anonymous = NOTIFYICONDATAW_0 {
        uVersion: NOTIFYICON_VERSION_4,
    };
    unsafe {
        if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
            // Happens at sign-in if Explorer isn't ready yet; TaskbarCreated
            // brings us back here once it is.
            log!("The taskbar isn't ready for the tray icon yet.");
        }
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
    }
    // Force the icon and tooltip to be set again.
    with_ctx(|ctx| ctx.logged.borrow_mut().take());
    refresh(hwnd);
}

fn describe(status: &Status) -> String {
    match status {
        Status::On => "Glide is on".into(),
        Status::Off => "Glide is off".into(),
        Status::PausedMinutes(m) if *m == 1 => "Paused for 1 more minute".into(),
        Status::PausedMinutes(m) => format!("Paused for {m} more minutes"),
        Status::Blocked(app) => format!("Paused while {app} runs"),
    }
}

/// Applies the shared state: installs or removes the hook, updates the icon and
/// tooltip, and (re)registers the hotkey.
fn refresh(hwnd: HWND) {
    with_ctx(|ctx| {
        let shared = &ctx.shared;
        let (status, hotkey_text) = {
            let state = lock(shared);
            (state.status(), state.config.toggle_hotkey.clone())
        };

        if status == Status::On {
            if shared.smoother.is_suspended() {
                if let Err(e) = shared.smoother.resume() {
                    log!("Couldn't reinstall the mouse hook: {e}");
                }
            }
        } else if !shared.smoother.is_suspended() {
            shared.smoother.suspend();
        }

        let text = describe(&status);
        let mut data = notify_data(hwnd);
        data.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.hIcon = if status == Status::On {
            ctx.icon_on
        } else {
            ctx.icon_off
        };
        let tip: Vec<u16> = format!("Glide\n{text}").encode_utf16().take(127).collect();
        data.szTip[..tip.len()].copy_from_slice(&tip);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }

        // One log line per kind of change, not one per minute of a timed pause.
        let key = match &status {
            Status::PausedMinutes(_) => "Paused for an hour".to_string(),
            other => describe(other),
        };
        if ctx.logged.borrow().as_deref() != Some(key.as_str()) {
            log!("{key}.");
            *ctx.logged.borrow_mut() = Some(key);
        }

        if *ctx.hotkey.borrow() != hotkey_text {
            unsafe {
                let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
            }
            if let Some(text) = &hotkey_text {
                match hotkey::parse(text) {
                    Ok(key) => match unsafe {
                        RegisterHotKey(Some(hwnd), HOTKEY_ID, key.modifiers, key.vk)
                    } {
                        Ok(()) => log!("Hotkey {text} turns Glide on and off."),
                        Err(_) => log!("Hotkey {text} is already used by another app."),
                    },
                    Err(e) => log!("Ignoring toggle_hotkey: {e}"),
                }
            }
            *ctx.hotkey.borrow_mut() = hotkey_text;
        }
    });
}

fn toggle(shared: &Shared) {
    let mut state = lock(shared);
    if state.paused_until.take().is_none() {
        state.enabled = !state.enabled;
    }
}

fn add_item(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    flags: MENU_ITEM_FLAGS,
    id: u32,
    text: &str,
) {
    unsafe {
        let _ = AppendMenuW(menu, MF_STRING | flags, id as usize, &HSTRING::from(text));
    }
}

fn show_menu(hwnd: HWND, at: POINT) {
    let Some(shared) = with_ctx(|ctx| ctx.shared.clone()) else {
        return;
    };
    let (status, enabled, paused, last_app, excluded) = {
        let state = lock(&shared);
        (
            state.status(),
            state.enabled,
            state.paused_until.is_some(),
            state.last_app.clone(),
            state.config.excluded_apps.clone(),
        )
    };

    let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    let none = MENU_ITEM_FLAGS(0);
    add_item(menu, MF_GRAYED, 0, &describe(&status));
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    }
    let toggle_text = if paused {
        "Resume now"
    } else if enabled {
        "Turn off"
    } else {
        "Turn on"
    };
    add_item(menu, none, ID_TOGGLE, toggle_text);
    if enabled && !paused {
        add_item(menu, none, ID_PAUSE, "Pause for 1 hour");
    }
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    }
    if let Some(app) = &last_app {
        if apps::is_built_in(app) {
            add_item(
                menu,
                MF_GRAYED | MF_CHECKED,
                0,
                &format!("Normal scrolling in {app} (built in)"),
            );
        } else {
            let checked = excluded.iter().any(|e| e.eq_ignore_ascii_case(app));
            let flags = if checked { MF_CHECKED } else { none };
            add_item(
                menu,
                flags,
                ID_EXCLUDE,
                &format!("Normal scrolling in {app}"),
            );
        }
    }
    add_item(menu, none, ID_SETTINGS, "Settings…");
    let auto = if autostart::is_enabled() {
        MF_CHECKED
    } else {
        none
    };
    add_item(menu, auto, ID_AUTOSTART, "Start with Windows");
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    }
    add_item(menu, none, ID_QUIT, "Quit Glide");

    let command = unsafe {
        // Without this the menu doesn't close when you click elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            at.x,
            at.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        chosen.0 as u32
    };
    handle_command(hwnd, &shared, command, last_app);
}

fn handle_command(hwnd: HWND, shared: &Shared, command: u32, last_app: Option<String>) {
    match command {
        ID_TOGGLE => toggle(shared),
        ID_PAUSE => lock(shared).paused_until = Some(Instant::now() + PAUSE_FOR),
        ID_EXCLUDE => {
            let Some(app) = last_app else { return };
            let mut state = lock(shared);
            let list = &mut state.config.excluded_apps;
            let before = list.len();
            list.retain(|e| !e.eq_ignore_ascii_case(&app));
            let now_excluded = list.len() == before;
            if now_excluded {
                list.push(app.clone());
            }
            shared.smoother.set_excluded_apps(&apps::excluded(list));
            match config::write_excluded_apps(&shared.config_path, list) {
                Ok(()) if now_excluded => log!("{app} now gets normal scrolling."),
                Ok(()) => log!("{app} is smoothed again."),
                Err(e) => log!("Couldn't save the exclusion for {app}: {e}"),
            }
        }
        ID_SETTINGS => {
            // A separate process, so the tray part stays small; it opens its
            // window or brings an already open one to the front.
            let spawned = std::env::current_exe()
                .and_then(|exe| std::process::Command::new(exe).arg("--settings").spawn());
            if let Err(e) = spawned {
                log!("Couldn't open the settings window: {e}");
            }
            return;
        }
        ID_AUTOSTART => {
            let on = !autostart::is_enabled();
            match autostart::set_enabled(on) {
                Ok(()) => log!("Start with Windows: {}.", if on { "on" } else { "off" }),
                Err(e) => log!("Couldn't change Start with Windows: {e}"),
            }
        }
        ID_QUIT => unsafe {
            let _ = DestroyWindow(hwnd);
            return;
        },
        _ => return,
    }
    refresh(hwnd);
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            // With NOTIFYICON_VERSION_4 the event is in the low word of lparam and
            // the click position is in wparam.
            let event = (lparam.0 & 0xffff) as u32;
            match event {
                NIN_SELECT | NIN_KEYSELECT => {
                    if let Some(shared) = with_ctx(|ctx| ctx.shared.clone()) {
                        toggle(&shared);
                        refresh(hwnd);
                    }
                }
                WM_CONTEXTMENU => {
                    let at = POINT {
                        x: (wparam.0 & 0xffff) as u16 as i16 as i32,
                        y: ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32,
                    };
                    show_menu(hwnd, at);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_REFRESH => {
            refresh(hwnd);
            LRESULT(0)
        }
        WM_POWERBROADCAST if wparam.0 == PBT_APMRESUMEAUTOMATIC => {
            // Hooks don't always survive sleep; put it back to be sure.
            if let Some(shared) = with_ctx(|ctx| ctx.shared.clone()) {
                if !shared.smoother.is_suspended() {
                    match shared.smoother.reinstall() {
                        Ok(()) => log!("Woke from sleep; mouse hook reinstalled."),
                        Err(e) => log!("Woke from sleep; couldn't reinstall the mouse hook: {e}"),
                    }
                }
            }
            LRESULT(1)
        }
        WM_HOTKEY => {
            if let Some(shared) = with_ctx(|ctx| ctx.shared.clone()) {
                toggle(&shared);
                refresh(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let data = notify_data(hwnd);
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ if with_ctx(|ctx| ctx.taskbar_created == msg).unwrap_or(false) => {
            add_icon(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// The background thread: reloads settings when the file is saved, pauses for
/// exam apps and anti-cheat games, ends timed pauses, and remembers which app
/// the user is in.
fn spawn_watcher(shared: Arc<Shared>, hwnd: HWND) {
    // HWND isn't Send; the handle value is all the other thread needs.
    let hwnd = hwnd.0 as isize;
    thread::Builder::new()
        .name("glide-watcher".into())
        .spawn(move || {
            let hwnd = HWND(hwnd as *mut _);
            let post = || unsafe {
                let _ = PostMessageW(Some(hwnd), WM_REFRESH, WPARAM(0), LPARAM(0));
            };
            let path = shared.config_path.clone();
            let mut seen = config::modified(&path).ok();
            let mut last_pause_check = Instant::now() - PAUSE_CHECK;
            let mut last_minute_tick = Instant::now();
            let mut last_skip = None;
            let mut last_hook_check = Instant::now();
            loop {
                let modified = config::modified(&path).ok();
                if modified != seen {
                    seen = modified;
                    match config::load(&path) {
                        Ok(fresh) => {
                            shared.smoother.set_params(fresh.params());
                            shared.smoother.set_options(fresh.options());
                            shared
                                .smoother
                                .set_excluded_apps(&apps::excluded(&fresh.excluded_apps));
                            log!("Settings reloaded. {}", fresh.summary());
                            lock(&shared).config = fresh;
                            last_pause_check = Instant::now() - PAUSE_CHECK;
                            post();
                        }
                        Err(e) => log!("Couldn't read settings, keeping the old ones: {e}"),
                    }
                }

                if last_pause_check.elapsed() >= PAUSE_CHECK {
                    last_pause_check = Instant::now();
                    let (exams, exam_apps, games) = {
                        let state = lock(&shared);
                        (
                            state.config.pause_during_exams,
                            state.config.exam_apps.clone(),
                            state.config.pause_during_anti_cheat_games,
                        )
                    };
                    let blocker = if exams || games {
                        glide_win::running_process_names().ok().and_then(|running| {
                            let exam = exams
                                .then(|| {
                                    pause::find_running(&running, pause::EXAM_APPS, &exam_apps)
                                })
                                .flatten();
                            let game = games
                                .then(|| {
                                    pause::find_running(&running, pause::ANTI_CHEAT_GAMES, &[])
                                })
                                .flatten();
                            exam.or(game).map(String::from)
                        })
                    } else {
                        None
                    };
                    let mut state = lock(&shared);
                    if state.blocked_by != blocker {
                        state.blocked_by = blocker;
                        drop(state);
                        post();
                    }
                }

                {
                    let mut state = lock(&shared);
                    if state
                        .paused_until
                        .is_some_and(|until| Instant::now() >= until)
                    {
                        state.paused_until = None;
                        drop(state);
                        post();
                    } else if state.paused_until.is_some()
                        && last_minute_tick.elapsed() >= Duration::from_secs(60)
                    {
                        // Keep the "N more minutes" tooltip current.
                        last_minute_tick = Instant::now();
                        drop(state);
                        post();
                    }
                }

                if last_hook_check.elapsed() >= HOOK_CHECK {
                    last_hook_check = Instant::now();
                    if shared.smoother.check_hook() {
                        log!("Windows dropped the mouse hook; reinstalled it.");
                    }
                }

                if let Some(app) = glide_win::foreground_app_name() {
                    lock(&shared).last_app = Some(app);
                }

                if shared.debug && !shared.smoother.is_suspended() {
                    let skip = shared.smoother.skip_reason_under_cursor();
                    if skip != last_skip {
                        let why = match skip {
                            None => "smoothing",
                            Some(Skip::Excluded) => "normal scrolling (on the exclusion list)",
                            Some(Skip::Elevated) => "normal scrolling (admin window)",
                            Some(Skip::FullscreenGame) => "normal scrolling (fullscreen game)",
                            Some(Skip::Wpf) => "normal scrolling (WPF app)",
                        };
                        log!("Under the cursor: {why}.");
                        last_skip = skip;
                    }
                }

                thread::sleep(Duration::from_millis(300));
            }
        })
        .expect("failed to spawn watcher thread");
}
