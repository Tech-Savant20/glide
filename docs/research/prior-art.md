# Prior art: smooth wheel scrolling

Research notes gathered on 2026-09-28 from the source of 20 open-source projects (shallow git clones, read directly) and from browser and OS documentation (scraped with Firecrawl). Facts are marked with the file or URL they came from. Where a project only *claims* something, it says so.

## Landscape

| Project | Platform / stack | Motion model | Output pacing |
|---|---|---|---|
| [gblazex/smoothscroll](https://github.com/gblazex/smoothscroll) (SmoothScroll for Chrome; same author as the commercial SmoothScroll app) | Browser extension, JS | Queue of per-notch animations, summed. Each follows Michael Herf's "pulse" curve over a fixed 400 ms | `requestAnimationFrame` |
| [maou-shonen/butter-scroll](https://github.com/maou-shonen/butter-scroll) | Windows, Rust + Tauri | Port of the pulse queue | Fixed frame sleep |
| [quangtruong2003/SmoothScroll](https://github.com/quangtruong2003/SmoothScroll) | Windows/macOS/Linux, Rust + Tauri | "Window payout": each notch paid out over its own window, overlapping windows add. Easing curve per window | Fixed 120 FPS |
| [bobo198504/SmoothWheelScroll](https://github.com/bobo198504/SmoothWheelScroll) | REAPER plugin, C++ | Origin of the window-payout model, with measured ripple numbers (below) | Host timer |
| [luingry/smoothmice](https://github.com/luingry/smoothmice) | Windows, C# | Pulse-queue superposition | 4 ms high-resolution waitable timer |
| [rafaelsg-01/soft-scroll](https://github.com/rafaelsg-01/soft-scroll) | Windows, C# WPF | Remaining-distance × ExponentialOut / Cubic / Quintic | Display Hz (floor 120), drops to 60 FPS after 2 s idle |
| [RaymondGuoCGI/SilkWheel](https://github.com/RaymondGuoCGI/SilkWheel) | Windows, C# | Pulse curve per notch, max 18 animations per axis | 7 ms timer |
| [zachey01/NimbusScroll](https://github.com/zachey01/NimbusScroll) | Windows/Wayland, Rust | Velocity + per-tick damping 0.975 every 4 ms (k ≈ 6.3/s) | 4 ms loop |
| [EricxWood/Scroil](https://github.com/EricxWood/Scroil) | Windows, C# | Sigmoid-remapped ring buffer | 1 ms |
| [EsportToys/LibreScroll](https://github.com/EsportToys/LibreScroll) | Windows | Middle-button drag → inertial scroll, friction 30/s | — |
| [Caldis/Mos](https://github.com/Caldis/Mos) | macOS, Swift | First-order chase: each frame moves `(target − current) × 0.085` (default) | `CVDisplayLink` (vsync) |
| [noah-nuebling/mac-mouse-fix](https://github.com/noah-nuebling/mac-mouse-fix) v3 | macOS, ObjC/Swift | "Hybrid curve": a base segment (linear, 140–300 ms per step) followed by drag physics `v' = −a·vᵇ` | Display link |
| [Stronautt/smooth-scroll-daemon](https://github.com/Stronautt/smooth-scroll-daemon) | Linux (VMs) | Rate-aware damping + exponential inertia | 250 Hz |
| [AshMartian/momentum_mouse](https://github.com/AshMartian/momentum_mouse), [Wayne6530/smooth-scroll-linux](https://github.com/Wayne6530/smooth-scroll-linux) | Linux uinput daemons | Physics-based inertia | — |
| Chromium | Browser | Ease-in-out cubic-bezier animation per wheel event, retargeted as more events arrive | Compositor |
| Firefox (`msdPhysics`) | Browser | Mass-spring-damper toward the target | Compositor |

Glide is the only Windows tool here that paces output to vsync. Mos does it on macOS with `CVDisplayLink`.

## Motion models in detail

### 1. Pulse queue (SmoothScroll / gblazex, butter-scroll, SmoothMice, SilkWheel)
`gblazex__smoothscroll/src/sscr.js`
- Every notch pushes `{distance, start}` into a queue. Each frame, every item advances along its own curve, and the outputs are **summed** (superposition).
- **Pulse curve** (Michael Herf, stereopsis.com/stopping), with `x = t·pulseScale`:
  - For `x < 1`: `x − (1 − e^(−x))`.
  - After that: `e^(−1) + (1 − e^(−(x−1)))·(1 − e^(−1))`.
  - The result is normalized so that `pulse(1) = 1`.
- The velocity starts at **zero** (a real ease-in) and peaks at `1/pulseScale` of the duration. The remainder is an exponential tail.
- Defaults: `animationTime` 400 ms, `stepSize` 100 px, `pulseScale` 4, `frameRate` 150.
- Acceleration: when notches are less than `accelerationDelta` (50 ms) apart, distance is multiplied by `(1 + 50/elapsed)/2`, capped at `accelerationMax` 3.
- SmoothMice's source comment gives the reason for superposition: a shared-state model "was structurally history-dependent… causing visible jumps/spikes when a notch landed mid-animation" (`SmoothMice.Core/Scrolling/SmoothScrollEngine.cs`).

### 2. Window payout (SmoothWheelScroll "model 3.0", adopted by quangtruong2003/SmoothScroll)
`bobo198504__SmoothWheelScroll/src/anim3_core.h`, `quangtruong2003__SmoothScroll/crates/core/src/window_model.rs`
- Each notch opens one window of fixed length and pays its amount out at a constant rate across it. Overlapping windows add. Total output equals total input exactly.
- **Measured ripple:** for a steady roll with a 150 ms gap and a 200 ms window, the output rate has about **35% standard deviation**. The cause is that the number of windows in flight alternates between n and n+1.
- Blending 50% smoothstep into each window's payout cuts the worst ripple from 35.4% to 18.9%.
- Easing only helps when windows overlap **and** window/gap is not close to a whole number (band ±0.08). When the ratio is a whole number, a constant rate is already flat, and easing made one case worse (0.1% → 11%).
- quangtruong2003 defaults: 144 px step, 220 ms, QuinticOut.
  - Acceleration comes from an EWMA of notch rate (α 0.3) and ramps up to 10× at 20 notches/s.

### 3. First-order chase (Mos; Glide's first engine)
`Caldis__Mos/Mos/ScrollCore/ScrollPoster.swift`, `Utils/Constants.swift`
- `buffer += step × speed` on every notch. Each display frame emits `(buffer − current) × trans`.
- The default is `trans = 1 − √(4.35/5.2) ≈ 0.085` per frame. At 60 Hz that's k ≈ 5.3/s, with 95% settled in about 560 ms.
- Because the rate is per frame, the feel changes with refresh rate.
- Optional "simulate trackpad" mode sends macOS scroll phases so apps apply their own native momentum. Windows has no equivalent for injected wheel input.

### 4. Hybrid base + drag (Mac Mouse Fix v3), the closest to Glide's current engine
`mmf-v3/Helper/Core/Config/ScrollConfig.swift`, `Shared/Math/Curves/HybridCurves.swift`, `DragCurve.swift`, `Helper/Core/Scroll/Scroll.m`
- On each tick, `remaining + new step` is re-planned as a **base curve**, then a **drag curve** is attached at the point where the total distance works out exactly.
  - The base curve is linear, i.e. constant speed, for `msPerStep` 140–300 ms.
  - The drag curve is physical deceleration `v' = −a·vᵇ`, with `dragExponent` 0.7–1.05, `dragCoefficient` 15–40 and `stopSpeed` 30–50.
- `speedSmoothing` makes "the initial speed of the baseCurve equal to the current speed … so the animation speed doesn't jump after a scrollwheel-tick occurs". This is the same problem Glide's second Scroll Lab report showed.
- Ticks count as consecutive when they are 15–160 ms apart. A "fast scroll" speed-up starts after repeated swipes.

### 5. Velocity + damping (NimbusScroll, momentum_mouse, smooth-scroll-daemon)
- Each notch adds velocity, which decays by a fixed factor every tick.
- smooth-scroll-daemon damps by input rate: no damping below 5 events/s, square-root interpolation up to 30/s, and 0.3× for flicks above that. Friction is 0.08 per 4 ms tick.

### Reference constants
- **Apple `UIScrollView.DecelerationRate`**: `.normal` = 0.998 per ms (time constant ≈ 500 ms), `.fast` = 0.99 per ms (≈ 100 ms). Position follows `target − amplitude·e^(−t/τ)` ([ariya.io](https://ariya.io/2011/10/flick-list-with-its-momentum-scrolling-and-deceleration/), [esskeetit on Medium](https://medium.com/@esskeetit/scrolling-mechanics-of-uiscrollview-142adee1142c)).
- **Firefox `msdPhysics`**: spring constants `motionBegin`, `regular` and `slowdown`. A popular tweak uses 300 / 900 / 300 ([axlefublr](https://axlefublr.github.io/smooth-scrolling/)).

## How Chromium handles our injected wheel events (important)

- **It never treats a Windows wheel event as precise.** `ui/events/blink/web_input_event_builders_win.cc`: *"we leave hasPreciseScrollingDeltas false, even for trackpad scrolls that generate WM_MOUSEWHEEL … (crbug.com/545234)"*.
  - It converts deltas at 100 px per 3 lines, i.e. about 0.83 px per wheel unit. Scroll Lab measured 0.8.
- **So every small delta Glide injects starts a Chromium smooth-scroll animation.**
  - The curve is `cc/animation/scroll_offset_animation_curve.cc`: ease-in-out `cubic-bezier(0.42, 0, 0.58, 1)`.
  - Duration uses `kInverseDelta`: 12 frames at 60 Hz (**200 ms**) for deltas ≤ 120 px, falling to 6 frames (100 ms) at 480 px. Small deltas get the longest animation.
  - Each new event retargets the running animation and preserves its velocity.
  - The result is that Glide's glide is smoothed a second time, adding lag and a rubbery feel.
- **Chromium turns this animation off** when any of these is true:
  - `chrome://flags/#smooth-scrolling` is Disabled, or Chrome runs with `--disable-smooth-scrolling`.
  - Windows **"Animation effects"** is off (`SPI_GETCLIENTAREAANIMATION`). Source: `ui/gfx/animation/animation_win.cc`, `ScrollAnimationsEnabledBySystem()`.
  - It's a remote desktop session.
- Commercial SmoothScroll and the open-source tools all smooth browsers anyway. quangtruong2003 notes "Browsers are intentionally not included [in auto-exclusion]: users expect SmoothScroll's configured feel there." None of them turn off Chromium's own animation.

## App compatibility tricks found in the wild

| Problem | Who handles it | How |
|---|---|---|
| **WPF apps scroll a full step per wheel event**, whatever the delta, so small deltas massively over-scroll | butter-scroll (`detector_win.rs`, `threshold.rs`) | Window class `HwndWrapper*` → "Legacy120" mode that buffers output until it reaches whole 120-unit notches. Auto-detects other windows with `WS_VSCROLL` by comparing scrollbar movement to the expected delta (> 5× means legacy). Results are cached per exe path + mtime |
| Apps with **their own smooth scrolling** get smoothed twice | quangtruong2003 (`settings.rs NATIVE_SMOOTH_SEED`) | Auto-disabled: `Notepad.exe`, `SystemSettings.exe`, `ApplicationFrameHost.exe`, `CalculatorApp.exe`, `Photos.exe`, `WinStore.App.exe` |
| Apps that need **whole notches** (DAWs) | quangtruong2003 | "Discrete notch preserving" strategy. Built-in: `reaper.exe` |
| Legacy apps mishandling tiny deltas | LibreScroll ("Minimum X/Y Step"), quangtruong2003 (`EMIT_UNIT = 12`) | Emit only multiples of a minimum step |
| **Games** | quangtruong2003, SmoothMice (`GameWindowClassifier`) | Default exclusion list (LoL, Valorant, CS2, Dota 2, Apex, Fortnite, GTA5, Minecraft `javaw.exe`, …) plus fullscreen detection |
| **Laptop touchpads** double-smoothed | Soft Scroll (`InputDeviceDetector.cs`) | Raw Input `RID_DEVICE_INFO.mouse.fTouchPad` marks touchpad devices, and smoothing is skipped for them |
| Shift+wheel horizontal in Figma / Pencil | quangtruong2003, Soft Scroll | `PostMessageW(WM_MOUSEWHEEL, MK_SHIFT, …)` **with the sign inverted**, instead of `MOUSEEVENTF_HWHEEL` |
| Modifier released mid-glide changes the meaning (zoom becomes scroll) | quangtruong2003 (`wheel_emitter.rs`) | Capture modifiers when the gesture starts. Re-synthesize a missing Ctrl/Shift/Alt around injected events. Cancel if an extra modifier is pressed |
| Word's scroll target is a child window (`_WwG` under `OpusApp`) | quangtruong2003 | Use `SendInput`: posting `WM_MOUSEWHEEL` to the root window never reaches the child |
| "Scroll inactive windows" setting; hook re-entry | SmoothMice (`ScrollInjector.cs`) | `PostMessage` to the hwnd under the cursor. It **claims** `SendInput` "bypasses UIPI" for elevated windows. That contradicts Microsoft's UIPI documentation, so treat it as unverified |
| Free-spin wheels (Logitech MX) | SmoothMice (`FreeSpinDetection.cs`), smooth-scroll-daemon | Detect free-spin bursts and suppress inertia. Emit both `REL_WHEEL_HI_RES` and `REL_WHEEL` at 120 boundaries (Linux) |
| Reduce motion | quangtruong2003 (`RespectReduceMotion`) | Follow the OS animation setting: Auto / Always / Never |

## What this means for Glide

1. **Glide's current engine is on the right track.** Moving at the finger's speed and then decelerating is essentially Mac Mouse Fix's hybrid (linear base + drag tail with `speedSmoothing`). That's the most refined open-source design found. It also avoids the window-payout ripple (35% at a 150/200 ms gap) and the pulse queue's per-notch spikes.
   - **Next step:** add a benchmark test that measures ripple as the standard deviation of per-frame output during a steady roll, the way SmoothWheelScroll does, across gaps from 80 to 300 ms. That gives Glide a number to beat and to report in the README.
2. **Chromium double-smoothing is the biggest remaining source of lag and "rubbery" feel in browsers.** Every injected delta triggers a 200 ms ease-in-out retarget. Options, in order:
   - A first-run tip, plus a settings-page check that links to `chrome://flags/#smooth-scrolling` / `edge://flags/#smooth-scrolling`.
   - Document that turning off Windows "Animation effects" also disables it, but that setting is system-wide.
   - Measure with Scroll Lab in both modes before deciding anything cleverer.
3. **Built-in exclusion lists at launch:**
   - Native-smooth UWP/WinUI apps (the list above).
   - Games.
   - `reaper.exe` as a whole-notch app.
4. **Add a "whole notches" output mode per app**, and auto-select it for WPF (`HwndWrapper*` window class). Without it, WPF apps such as Visual Studio's WPF panes and many .NET tools will over-scroll badly.
5. **Skip smoothing for touchpads** detected through Raw Input `fTouchPad`. Glide already passes sub-120 deltas through, but legacy non-precision touchpads send whole notches.
6. **Capture modifiers when a gesture starts** so Shift/Ctrl tails don't change meaning when the key is released.
7. **Offer a minimum emit step** (for example 12 units) as a per-app compatibility knob.
8. **Keep vsync pacing.** It's a differentiator, since every other Windows tool uses a fixed 4–8 ms timer.
