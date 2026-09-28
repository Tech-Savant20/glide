use std::sync::mpsc::Receiver;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use glide_engine::{Engine, Params};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
use windows::Win32::System::Threading::{
    CreateWaitableTimerExW, GetCurrentThread, SetThreadPriority, SetWaitableTimer,
    WaitForSingleObject, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE,
    THREAD_PRIORITY_HIGHEST, TIMER_ALL_ACCESS,
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
    let timer = FrameTimer::new();
    let mut engine = Engine::new(params);
    let mut clock = EventClock::default();
    let mut frame = Duration::from_secs_f64(1.0 / 60.0);
    let mut last_frame = Instant::now();

    loop {
        if !engine.is_active() {
            // Idle: sleep until input arrives, costing no CPU.
            let Ok(msg) = rx.recv() else { return };
            if !handle(msg, &mut engine, &mut clock) {
                return;
            }
            frame = display_frame();
            last_frame = Instant::now();
        }
        for msg in rx.try_iter() {
            if !handle(msg, &mut engine, &mut clock) {
                return;
            }
        }
        if !engine.is_active() {
            continue;
        }

        timer.wait(frame);
        let now = Instant::now();
        let dt = (now - last_frame).as_secs_f64().min(MAX_FRAME);
        last_frame = now;
        inject::send_wheel(engine.tick(dt));
    }
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
