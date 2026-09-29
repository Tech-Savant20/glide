use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};

use glide_engine::{Axis, NOTCH};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HC_ACTION, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT,
    WH_MOUSE_LL, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_QUIT,
    WM_RBUTTONDOWN, WM_XBUTTONDOWN,
};

use crate::{target, Msg};

pub(crate) static ENABLED: AtomicBool = AtomicBool::new(true);
/// Every mouse event the hook has seen, including moves. The watchdog compares it
/// with cursor movement to notice when Windows has dropped the hook.
pub(crate) static EVENTS: AtomicU64 = AtomicU64::new(0);
static NATURAL: AtomicBool = AtomicBool::new(false);
static SMOOTH_HIRES: AtomicBool = AtomicBool::new(false);
static SENDER: OnceLock<Sender<Msg>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Invert scroll direction, like macOS "natural scrolling".
    pub natural: bool,
    /// Also smooth wheels that already send sub-notch deltas (free-spin or hi-res
    /// wheels). Off by default because they are already fine-grained.
    pub smooth_hires: bool,
}

pub(crate) fn set_options(options: Options) {
    NATURAL.store(options.natural, Relaxed);
    SMOOTH_HIRES.store(options.smooth_hires, Relaxed);
}

pub(crate) struct HookThread {
    thread_id: u32,
    join: JoinHandle<()>,
}

/// Connects the hook callback to the animator. Must be called once, before the
/// first [`HookThread::spawn`].
pub(crate) fn connect(tx: Sender<Msg>) {
    SENDER
        .set(tx)
        .expect("only one Smoother may run per process");
}

impl HookThread {
    /// Installs the mouse hook on a new thread. Can be called again after
    /// [`HookThread::stop`] to reinstall it.
    pub(crate) fn spawn() -> windows::core::Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name("glide-hook".into())
            .spawn(move || run(ready_tx))
            .expect("failed to spawn hook thread");

        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(Self { thread_id, join }),
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => panic!("hook thread exited during startup"),
        }
    }

    pub(crate) fn stop(self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        let _ = self.join.join();
    }
}

fn run(ready: Sender<windows::core::Result<u32>>) {
    let hook = unsafe {
        // A slow hook callback delays every mouse event on the system.
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
        GetModuleHandleW(None).and_then(|module| {
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module.into()), 0)
        })
    };
    let hook = match hook {
        Ok(hook) => hook,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    EVENTS.fetch_add(1, Relaxed);
    if code == HC_ACTION as i32 && ENABLED.load(Relaxed) {
        // SAFETY: for WH_MOUSE_LL with HC_ACTION, lparam points at an MSLLHOOKSTRUCT.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if let Some(msg) = classify(wparam.0 as u32, info) {
            let swallow = matches!(msg, Msg::Wheel { .. });
            let sent = SENDER.get().is_some_and(|tx| tx.send(msg).is_ok());
            // Only swallow the notch once the animator has it, so a dead animator
            // degrades to normal scrolling rather than no scrolling.
            if swallow && sent {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn classify(message: u32, info: &MSLLHOOKSTRUCT) -> Option<Msg> {
    // Injected input includes our own replayed deltas; never smooth it twice.
    if info.flags & LLMHF_INJECTED != 0 {
        return None;
    }
    match message {
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let delta = (info.mouseData >> 16) as u16 as i16 as f64;
            if delta == 0.0 || (delta % NOTCH != 0.0 && !SMOOTH_HIRES.load(Relaxed)) {
                return None;
            }
            // Ctrl+wheel is zoom almost everywhere; stepped zoom is what users expect.
            if key_down(VK_CONTROL.0) {
                return None;
            }
            if target::skip_reason(info.pt).is_some() {
                return None;
            }
            let mut axis = if message == WM_MOUSEWHEEL {
                Axis::Vertical
            } else {
                Axis::Horizontal
            };
            let mut notches = delta / NOTCH;
            if axis == Axis::Vertical && key_down(VK_SHIFT.0) {
                // Wheel down (negative) scrolls right (positive horizontal).
                axis = Axis::Horizontal;
                notches = -notches;
            }
            if NATURAL.load(Relaxed) {
                notches = -notches;
            }
            Some(Msg::Wheel {
                axis,
                notches,
                time_ms: info.time,
                window: target::window_at(info.pt),
            })
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => Some(Msg::Stop),
        _ => None,
    }
}

fn key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) < 0 }
}
