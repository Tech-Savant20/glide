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

use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;

use glide_engine::{Axis, Params};

pub use hook::Options;

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
    hook: hook::HookThread,
    animator: JoinHandle<()>,
}

impl Smoother {
    pub fn start(params: Params, options: Options) -> windows::core::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let animator = animator::spawn(rx, params);
        hook::set_options(options);
        let hook = hook::HookThread::spawn(tx.clone())?;
        Ok(Self { tx, hook, animator })
    }

    /// Turns smoothing on or off. While off, wheel events pass through untouched.
    pub fn set_enabled(&self, enabled: bool) {
        hook::ENABLED.store(enabled, Relaxed);
        if !enabled {
            let _ = self.tx.send(Msg::Stop);
        }
    }

    pub fn is_enabled(&self) -> bool {
        hook::ENABLED.load(Relaxed)
    }

    pub fn set_params(&self, params: Params) {
        let _ = self.tx.send(Msg::SetParams(params));
    }

    pub fn set_options(&self, options: Options) {
        hook::set_options(options);
    }

    pub fn shutdown(self) {
        self.hook.stop();
        let _ = self.tx.send(Msg::Quit);
        let _ = self.animator.join();
    }
}
