//! Apps that get raw wheel events out of the box. Users add more with
//! `excluded_apps` in the config.
//!
//! Sources for these lists are in docs/research/prior-art.md.

/// Windows apps that already animate wheel scrolling themselves (WinUI/UWP), so
/// smoothing them again adds lag.
const NATIVE_SMOOTH: &[&str] = &[
    "Notepad.exe",
    "SystemSettings.exe",
    "ApplicationFrameHost.exe",
    "CalculatorApp.exe",
    "Photos.exe",
    "WinStore.App.exe",
];

/// Apps where one wheel notch must stay one notch (for example, a DAW nudging
/// by one step per notch), or that ignore deltas smaller than a notch (many Java
/// Swing apps read the wheel as whole clicks, so small deltas scroll nothing).
const WHOLE_NOTCH: &[&str] = &["reaper.exe", "java.exe"];

/// Games, where the wheel usually switches weapons or zooms. Fullscreen games are
/// also caught by the fullscreen check; this covers windowed and borderless ones.
const GAMES: &[&str] = &[
    "LeagueOfLegends.exe",
    "VALORANT.exe",
    "VALORANT-Win64-Shipping.exe",
    "csgo.exe",
    "cs2.exe",
    "dota2.exe",
    "r5apex.exe",
    "RainbowSix.exe",
    "FortniteClient-Win64-Shipping.exe",
    "TslGame.exe",
    "GTA5.exe",
    "RDR2.exe",
    "eldenring.exe",
    "Cyberpunk2077.exe",
    "witcher3.exe",
    "javaw.exe",
    "Minecraft.Windows.exe",
    "RocketLeague.exe",
    "Overwatch.exe",
    "Wow.exe",
    "ffxiv_dx11.exe",
    "Warframe.x64.exe",
    "factorio.exe",
    "Terraria.exe",
    "StardewValley.exe",
    "eurotrucks2.exe",
    "amtrucks.exe",
    "RobloxPlayerBeta.exe",
];

/// The built-in list plus the user's additions.
pub fn excluded(user: &[String]) -> Vec<String> {
    NATIVE_SMOOTH
        .iter()
        .chain(WHOLE_NOTCH)
        .chain(GAMES)
        .map(|s| s.to_string())
        .chain(user.iter().cloned())
        .collect()
}
