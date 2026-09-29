# Glide

Mac-style smooth scrolling for Windows mouse wheels.

A normal mouse wheel on Windows jumps three lines per notch. Glide turns each notch into smooth motion, the way scrolling feels on a Mac: it eases in and out, keeps a steady speed while you roll the wheel, and coasts after a quick flick.

> **Status:** early (v0.1). It works well day to day, but expect rough edges, and please [report them](https://github.com/Tech-Savant20/glide/issues).

## Features

- **Smooth scrolling everywhere**, with a steady speed while you keep rolling instead of a surge on every notch. See [how it compares](#how-it-compares).
- **Momentum:** a quick flick keeps coasting and slows down at the same rate as macOS (Apple's `0.998` per millisecond).
- **Presets** (macOS Magic Mouse, macOS Trackpad, Subtle, Snappy) plus sliders for distance, glide time, coasting, acceleration and top speed, with a live preview.
- **Natural scrolling** (reversed direction) as an option.
- **Timed to your display:** Glide sends one small step per screen refresh, so the motion lines up with what you see.
- **Leaves alone the apps it shouldn't touch:**
  - admin windows
  - fullscreen games
  - WPF apps
  - apps that already scroll smoothly (such as the new Notepad and Settings)
  - Java and REAPER, which need whole notches
  - common games
  - anything you add yourself
- **Steps aside completely** while exam lockdown software (Safe Exam Browser, Respondus LockDown Browser, Examplify, Guardian Browser, Pearson OnVUE, Inspera) or games with anti-cheat (Riot Vanguard, Easy Anti-Cheat, BattlEye) are running. Glide removes its mouse hook and comes back when they close. Both can be turned off.
- **Small:** the background program is a sub-megabyte exe using about 2 MB of memory; the settings window is a separate program that only runs while it's open. No admin rights needed.

## Install

Download the latest version from the [Releases](https://github.com/Tech-Savant20/glide/releases/latest) page:

- **`Glide-<version>-setup.exe`**: installs for your user account only (no admin prompt), adds Start menu entries, and can start Glide when you sign in.
- **`Glide-<version>-portable.zip`**: unzip anywhere and run `glide.exe`.

Each release lists SHA-256 checksums in `SHA256SUMS.txt`.

> **Windows may warn you the first time you run it** ("Windows protected your PC"). Releases aren't code-signed yet; that's in progress. Click **More info → Run anyway**, or check the file against `SHA256SUMS.txt` first. winget and Scoop packages will follow once releases are signed.

Requires Windows 10 version 1903 or later, or Windows 11 (64-bit).

After starting, Glide lives in the notification area next to the clock. Windows 11 may hide new icons behind the **^** arrow; drag it onto the taskbar to keep it visible.

## Using Glide

- **Tray icon:** blue means on; grey means off or paused. **Left-click** turns Glide on or off. **Right-click** shows:
  - pause for an hour
  - "Normal scrolling in *this app*"
  - settings
  - start with Windows
  - quit
- **Settings** (`glide-settings.exe`, the *Glide Settings* Start menu entry, or *Settings…* in the tray menu): changes apply as soon as you make them.
- **Settings file:** `%APPDATA%\Glide\config.toml`. The settings window writes it, but you can also edit it by hand, and Glide reloads it when it's saved.
- **Log:** `%LOCALAPPDATA%\Glide\glide.log`. Run `glide.exe --debug` from a terminal to see it live, including whether the window under the cursor is being smoothed.

### Chrome and Edge

Chromium browsers add their own smooth-scrolling animation to every wheel event, so on top of Glide they can feel slightly rubbery. For the most direct feel, turn it off at `chrome://flags/#smooth-scrolling` (or `edge://flags/#smooth-scrolling`).

## Privacy

Glide collects nothing. It has no telemetry and no analytics, and it makes no network requests. It reads wheel events only to replay them smoothly, and it never records or sends them anywhere.

## How it compares

"Ripple" is how much the scroll speed wobbles while you roll the wheel at a steady pace (standard deviation over mean); lower is smoother. These figures come from Glide's own benchmark, which also re-implements the models other tools use (details in [docs/research/prior-art.md](docs/research/prior-art.md)):

| Time between notches | Glide (60 Hz) | Per-notch animation (SmoothScroll's model) | Fixed 200 ms window |
|---|---|---|---|
| 120 ms | 2.7% | 7.1% | 29.0% |
| 200 ms | 0.5% | 30.7% | 30.1% |
| 300 ms | 0.4% | 54.4% | 76.2% |

## Building

You need Rust (stable, with the MSVC toolchain) and the Visual Studio C++ Build Tools.

```powershell
cargo test --workspace
cargo build --release          # target\release\glide.exe
.\scripts\package.ps1          # dist\: portable zip, installer (needs Inno Setup 6), checksums
```

The workspace has three crates:

- `crates/glide-engine`: the scroll physics. Pure Rust, no Windows code, heavily tested.
- `crates/glide-win`: the Windows side: the low-level mouse hook, per-window rules, and a vsync-paced injector.
- `crates/glide-app`: the app: tray, settings file, settings window, pause rules.

`tools/scroll-lab.html` is a page that measures scroll smoothness frame by frame in a browser.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

The settings window is built with [Slint](https://slint.dev), used under the Slint Royalty-free License, which asks for this credit: **Made with Slint**.
