//! Decides, per wheel event, whether the window under the cursor gets smoothed
//! scrolling or the raw wheel event.
//!
//! Runs inside the low-level hook callback, so everything here must be quick:
//! per-event checks are local user32 calls, and the slower per-process lookups
//! (executable name, elevation) are cached per process on the hook thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{OnceLock, PoisonError, RwLock};
use std::time::{Duration, Instant};

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetCursorInfo, GetWindowLongW, GetWindowRect,
    GetWindowThreadProcessId, IsZoomed, WindowFromPoint, CURSORINFO, CURSOR_SHOWING, GA_ROOT,
    GWL_STYLE, WS_CAPTION,
};

/// Why a window gets raw wheel events instead of smoothed ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    /// On the built-in or user exclusion list.
    Excluded,
    /// Runs with higher privileges than Glide, so Windows would drop our
    /// injected input (UIPI) and the wheel would stop working there.
    Elevated,
    /// A fullscreen window with the cursor hidden: almost always a game, where
    /// the wheel switches weapons or zooms rather than scrolls.
    FullscreenGame,
    /// WPF scrolls a full step for every wheel event whatever its size, so a
    /// stream of small deltas would scroll many times too far.
    Wpf,
}

/// Executable names (lowercase) that always get raw wheel events.
static EXCLUDED: RwLock<Vec<String>> = RwLock::new(Vec::new());
/// Bumped whenever [`EXCLUDED`] changes, to invalidate cached verdicts.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Cached per-process facts are refreshed this often, which also covers a
/// process id being reused by a new process.
const PROCESS_TTL: Duration = Duration::from_secs(5);

pub(crate) fn set_excluded(names: &[String]) {
    let names = names
        .iter()
        .map(|n| n.trim().to_lowercase())
        .filter(|n| !n.is_empty())
        .collect();
    *EXCLUDED.write().unwrap_or_else(PoisonError::into_inner) = names;
    GENERATION.fetch_add(1, Relaxed);
}

struct ProcessFacts {
    skip: Option<Skip>,
    checked: Instant,
    generation: u64,
}

thread_local! {
    static PROCESSES: RefCell<HashMap<u32, ProcessFacts>> = RefCell::new(HashMap::new());
}

/// Returns why the window under `point` should get raw wheel events, or `None`
/// to smooth it.
pub(crate) fn skip_reason(point: POINT) -> Option<Skip> {
    unsafe {
        let hwnd = WindowFromPoint(point);
        if hwnd.is_invalid() {
            return None;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        let root = if root.is_invalid() { hwnd } else { root };

        if is_wpf(hwnd) || is_wpf(root) {
            return Some(Skip::Wpf);
        }
        if is_fullscreen_game(root) {
            return Some(Skip::FullscreenGame);
        }

        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        process_skip(pid)
    }
}

fn process_skip(pid: u32) -> Option<Skip> {
    let generation = GENERATION.load(Relaxed);
    PROCESSES.with_borrow_mut(|cache| {
        if let Some(facts) = cache.get(&pid) {
            if facts.generation == generation && facts.checked.elapsed() < PROCESS_TTL {
                return facts.skip;
            }
        }
        if cache.len() > 256 {
            cache.retain(|_, f| f.checked.elapsed() < PROCESS_TTL);
        }
        let skip = inspect_process(pid);
        cache.insert(
            pid,
            ProcessFacts {
                skip,
                checked: Instant::now(),
                generation,
            },
        );
        skip
    })
}

fn inspect_process(pid: u32) -> Option<Skip> {
    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    else {
        // Even limited queries are refused for protected and some elevated
        // processes. Assume the worst: raw events always work.
        return Some(Skip::Elevated);
    };
    let skip = (|| {
        if let Some(exe) = exe_name(process) {
            let excluded = EXCLUDED.read().unwrap_or_else(PoisonError::into_inner);
            if excluded.contains(&exe) {
                return Some(Skip::Excluded);
            }
        }
        if !self_elevated() && is_elevated(process).unwrap_or(true) {
            return Some(Skip::Elevated);
        }
        None
    })();
    unsafe {
        let _ = CloseHandle(process);
    }
    skip
}

/// Lowercase file name of the process's executable, like `chrome.exe`.
fn exe_name(process: HANDLE) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .ok()?;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    let name = path.rsplit(['\\', '/']).next()?;
    Some(name.to_lowercase())
}

fn is_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok.then_some(elevation.TokenIsElevated != 0)
    }
}

fn self_elevated() -> bool {
    static SELF: OnceLock<bool> = OnceLock::new();
    *SELF.get_or_init(|| is_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false))
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn is_wpf(hwnd: HWND) -> bool {
    class_name(hwnd).starts_with("HwndWrapper")
}

fn is_fullscreen_game(root: HWND) -> bool {
    unsafe {
        let mut cursor = CURSORINFO {
            cbSize: size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        // A visible cursor means a normal app, or a fullscreen video or slideshow.
        if GetCursorInfo(&mut cursor).is_err() || cursor.flags.0 & CURSOR_SHOWING.0 != 0 {
            return false;
        }
        let class = class_name(root);
        if class == "Progman" || class == "WorkerW" {
            return false; // the desktop itself
        }
        // Games run borderless or exclusive fullscreen. A maximized normal window
        // keeps its title bar, and Windows also hides the cursor while you type in
        // one, so both checks are needed to avoid mistaking an editor for a game.
        let style = GetWindowLongW(root, GWL_STYLE) as u32;
        if style & WS_CAPTION.0 == WS_CAPTION.0 || IsZoomed(root).as_bool() {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(root, &mut rect).is_err() {
            return false;
        }
        let monitor = MonitorFromWindow(root, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
        let m = info.rcMonitor;
        rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom
    }
}
