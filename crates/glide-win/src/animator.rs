use std::sync::mpsc::Receiver;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use glide_engine::{Engine, Params};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Graphics::Dwm::{DwmFlush, DwmGetCompositionTimingInfo, DWM_TIMING_INFO};
use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
use windows::Win32::System::Performance::QueryPerformanceFrequency;
use windows::Win32::System::Threading::{
    CreateWaitableTimerExW, GetCurrentThread, SetThreadPriority, SetWaitableTimer,
    WaitForSingleObject, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, THREAD_PRIORITY_HIGHEST,
    TIMER_ALL_ACCESS,
};

use crate::{inject, Msg};

/// Frames longer than this (a stall or a debugger pause) are clamped so motion
/// does not jump.
const MAX_FRAME: f64 = 0.05;

pub(crate) fn spawn(rx: Receiver<Msg>, params: Params) -> JoinHandle<()> {
    thread::Builder::new()
        .name("glide-animator".into())
        .spawn(move || run(rx, params))
        .expect("failed to spawn animator thread")
}

fn run(rx: Receiver<Msg>, params: Params) {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
    }
    let mut pacer = Pacer::new();
    let mut engine = Engine::new(params);
    let mut clock = EventClock::default();

    loop {
        if !engine.is_active() {
            // Idle: sleep until input arrives, costing no CPU.
            let Ok(msg) = rx.recv() else { return };
            if !handle(msg, &mut engine, &mut clock) {
                return;
            }
            if !engine.is_active() {
                continue;
            }
            // Emit the first frame now rather than after the next vblank, which
            // removes up to a frame of latency from every scroll.
            let frame = pacer.restart();
            inject::send_wheel(engine.tick(frame));
        }
        for msg in rx.try_iter() {
            if !handle(msg, &mut engine, &mut clock) {
                return;
            }
        }
        if !engine.is_active() {
            continue;
        }

        let dt = pacer.next_frame();
        inject::send_wheel(engine.tick(dt));
    }
}

/// Paces output to the compositor so every displayed frame receives exactly one
/// delta. A free-running timer drifts against vsync, so some frames would get two
/// deltas and others none, which reads as judder.
struct Pacer {
    timer: FrameTimer,
    frame: Duration,
    qpc_per_second: f64,
    last_vblank: Option<u64>,
    last_wake: Instant,
}

impl Pacer {
    fn new() -> Self {
        let mut qpc_per_second = 0i64;
        unsafe {
            let _ = QueryPerformanceFrequency(&mut qpc_per_second);
        }
        Self {
            timer: FrameTimer::new(),
            frame: display_frame(),
            qpc_per_second: qpc_per_second.max(1) as f64,
            last_vblank: None,
            last_wake: Instant::now(),
        }
    }

    /// Starts a new motion and returns the nominal frame length in seconds.
    fn restart(&mut self) -> f64 {
        self.frame = display_frame();
        self.last_vblank = None;
        self.last_wake = Instant::now();
        self.frame.as_secs_f64()
    }

    /// Blocks until the next frame and returns its length in seconds.
    fn next_frame(&mut self) -> f64 {
        if let Some(vblank) = wait_for_vblank() {
            self.last_wake = Instant::now();
            match self.last_vblank.replace(vblank) {
                // Vblank timestamps are exact, so frame lengths carry no wake-up jitter.
                Some(prev) if vblank > prev => {
                    return ((vblank - prev) as f64 / self.qpc_per_second).min(MAX_FRAME)
                }
                None => return self.frame.as_secs_f64(),
                // Same vblank: DWM did not block (for example, nothing is being
                // composed). Fall back to the timer so we don't spin.
                Some(_) => {}
            }
        }
        self.last_vblank = None;
        self.timer.wait(self.frame);
        let now = Instant::now();
        let dt = (now - self.last_wake).as_secs_f64().min(MAX_FRAME);
        self.last_wake = now;
        dt
    }
}

/// Blocks until the compositor presents, then returns that vblank's QPC timestamp.
fn wait_for_vblank() -> Option<u64> {
    let mut info = DWM_TIMING_INFO {
        cbSize: size_of::<DWM_TIMING_INFO>() as u32,
        ..Default::default()
    };
    unsafe {
        DwmFlush().ok()?;
        // Since Windows 8.1 the hwnd must be null: timing is for the whole desktop.
        DwmGetCompositionTimingInfo(HWND::default(), &mut info).ok()?;
    }
    Some(info.qpcVBlank)
}

/// Applies one message. Returns false when the thread should exit.
fn handle(msg: Msg, engine: &mut Engine, clock: &mut EventClock) -> bool {
    match msg {
        Msg::Wheel {
            axis,
            notches,
            time_ms,
        } => engine.on_notch(axis, notches, clock.seconds(time_ms)),
        Msg::Stop => engine.stop(),
        Msg::SetParams(params) => engine.set_params(params),
        Msg::Quit => return false,
    }
    true
}

/// Turns the 32-bit millisecond event timestamps (which wrap every 49.7 days) into
/// monotonic seconds.
#[derive(Default)]
struct EventClock {
    last: Option<u32>,
    seconds: f64,
}

impl EventClock {
    fn seconds(&mut self, time_ms: u32) -> f64 {
        if let Some(last) = self.last {
            self.seconds += time_ms.wrapping_sub(last) as f64 / 1000.0;
        }
        self.last = Some(time_ms);
        self.seconds
    }
}

/// One refresh interval of the primary display.
fn display_frame() -> Duration {
    let mut mode = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    let ok = unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut mode) }.as_bool();
    // 0 and 1 mean "hardware default".
    let hz = if ok && mode.dmDisplayFrequency > 1 {
        mode.dmDisplayFrequency
    } else {
        60
    };
    Duration::from_secs_f64(1.0 / hz as f64)
}

/// A high-resolution waitable timer, which sleeps with sub-millisecond accuracy
/// without raising the system timer resolution.
struct FrameTimer(Option<HANDLE>);

impl FrameTimer {
    fn new() -> Self {
        let handle = unsafe {
            CreateWaitableTimerExW(
                None,
                None,
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )
        };
        Self(handle.ok())
    }

    fn wait(&self, duration: Duration) {
        let Some(handle) = self.0 else {
            thread::sleep(duration);
            return;
        };
        // Negative due time means relative, in 100 ns units.
        let due = -((duration.as_nanos() / 100) as i64);
        unsafe {
            if SetWaitableTimer(handle, &due, 0, None, None, false).is_ok() {
                WaitForSingleObject(handle, INFINITE);
            } else {
                thread::sleep(duration);
            }
        }
    }
}

impl Drop for FrameTimer {
    fn drop(&mut self) {
        if let Some(handle) = self.0 {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a live desktop, so it is opt-in: `cargo test -p glide-win -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn pacer_tracks_the_display_refresh() {
        let mut pacer = Pacer::new();
        let nominal = pacer.restart();
        let frames: Vec<f64> = (0..240).map(|_| pacer.next_frame()).skip(1).collect();
        let mean = frames.iter().sum::<f64>() / frames.len() as f64;
        let worst = frames
            .iter()
            .fold(0.0f64, |w, f| w.max((f - nominal).abs()));
        println!(
            "nominal {:.3} ms, mean {:.3} ms, worst deviation {:.3} ms, vblank pacing {}",
            nominal * 1000.0,
            mean * 1000.0,
            worst * 1000.0,
            if wait_for_vblank().is_some() {
                "on"
            } else {
                "off"
            },
        );
        assert!((mean - nominal).abs() < nominal * 0.1);
    }
}
