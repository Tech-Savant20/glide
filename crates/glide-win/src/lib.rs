//! Windows side of Glide: a low-level mouse hook that swallows wheel notches and an
//! animation thread that replays them as a stream of small wheel deltas.
//!
//! Two threads:
//! - the hook thread owns `WH_MOUSE_LL` and a message loop. Its callback must return
//!   fast (Windows silently drops slow low-level hooks), so it only classifies the
//!   event and forwards it over a channel.
//! - the animation thread owns the [`Engine`], wakes once per display frame while
//!   motion is in flight, and injects the output with `SendInput`.

mod animator;
mod hook;
mod inject;
mod processes;
mod target;

use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;

use glide_engine::{Axis, Params};

pub use hook::Options;
pub use processes::{foreground_app_name, running_process_names, visible_window_apps};
pub use target::Skip;

/// Messages to the animation thread.
#[derive(Debug)]
enum Msg {
    Wheel { axis: Axis, notches: f64, time_ms: u32 },
    Stop,
    SetParams(Params),
    Quit,
}

/// A running smoother. Only one may exist per process, because the hook callback
/// has no user data and reaches its state through statics.
pub struct Smoother {
    tx: Sender<Msg>,
    /// `None` while suspended: the hook is fully removed, not just bypassed.
    hook: Mutex<Option<hook::HookThread>>,
    animator: JoinHandle<()>,
}

impl Smoother {
    pub fn start(params: Params, options: Options) -> windows::core::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let animator = animator::spawn(rx, params);
        hook::set_options(options);
        hook::connect(tx.clone());
        let hook = hook::HookThread::spawn()?;
        Ok(Self {
            tx,
            hook: Mutex::new(Some(hook)),
            animator,
        })
    }

    /// Turns smoothing on or off. While off, wheel events pass through untouched
    /// but the hook stays installed; see [`Smoother::suspend`] to remove it.
    pub fn set_enabled(&self, enabled: bool) {
        hook::ENABLED.store(enabled, Relaxed);
        if !enabled {
            let _ = self.tx.send(Msg::Stop);
        }
    }

    pub fn is_enabled(&self) -> bool {
        hook::ENABLED.load(Relaxed)
    }

    /// Removes the mouse hook entirely, so Glide no longer sees or injects any
    /// input, until [`Smoother::resume`].
    pub fn suspend(&self) {
        let hook = self.hook.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(hook) = hook {
            hook.stop();
        }
        let _ = self.tx.send(Msg::Stop);
    }

    /// Reinstalls the mouse hook after [`Smoother::suspend`].
    pub fn resume(&self) -> windows::core::Result<()> {
        let mut hook = self.hook.lock().unwrap_or_else(PoisonError::into_inner);
        if hook.is_none() {
            *hook = Some(hook::HookThread::spawn()?);
        }
        Ok(())
    }

    pub fn is_suspended(&self) -> bool {
        self.hook
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
    }

    pub fn set_params(&self, params: Params) {
        let _ = self.tx.send(Msg::SetParams(params));
    }

    pub fn set_options(&self, options: Options) {
        hook::set_options(options);
    }

    /// Executable names (like `game.exe`) that always get raw wheel events.
    pub fn set_excluded_apps(&self, names: &[String]) {
        target::set_excluded(names);
    }

    /// Why the window under the mouse cursor would get raw wheel events, or
    /// `None` if Glide smooths it.
    pub fn skip_reason_under_cursor(&self) -> Option<Skip> {
        let mut point = windows::Win32::Foundation::POINT::default();
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point).ok()? };
        target::skip_reason(point)
    }

    pub fn shutdown(self) {
        self.suspend();
        let _ = self.tx.send(Msg::Quit);
        let _ = self.animator.join();
    }
}
