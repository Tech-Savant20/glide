use std::mem::size_of;

use glide_engine::Axis;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL, MOUSEINPUT,
};

/// Tags our injected events so other tools can recognise them ("GLID").
pub(crate) const GLIDE_TAG: usize = 0x474C_4944;

/// Sends wheel deltas for both axes in one `SendInput` call.
pub(crate) fn send_wheel(deltas: [i32; 2]) {
    let mut inputs = [INPUT::default(); 2];
    let mut count = 0;
    for (axis, delta) in [(Axis::Vertical, deltas[0]), (Axis::Horizontal, deltas[1])] {
        if delta == 0 {
            continue;
        }
        let flags = match axis {
            Axis::Vertical => MOUSEEVENTF_WHEEL,
            Axis::Horizontal => MOUSEEVENTF_HWHEEL,
        };
        inputs[count] = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    mouseData: delta as _,
                    dwFlags: flags,
                    dwExtraInfo: GLIDE_TAG,
                    ..Default::default()
                },
            },
        };
        count += 1;
    }
    if count > 0 {
        unsafe {
            SendInput(&inputs[..count], size_of::<INPUT>() as i32);
        }
    }
}
