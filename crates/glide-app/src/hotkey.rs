//! Parsing hotkeys like `Ctrl+Alt+G` for `RegisterHotKey`.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub modifiers: HOT_KEY_MODIFIERS,
    pub vk: u32,
}

/// Parses `Ctrl+Alt+G`, `Win+Shift+F9` and so on. At least one modifier is
/// required, so a hotkey can never swallow a plain key.
pub fn parse(text: &str) -> Result<Hotkey, String> {
    let mut modifiers = MOD_NOREPEAT;
    let mut has_modifier = false;
    let mut vk = None;
    for part in text.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "alt" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "windows" => modifiers |= MOD_WIN,
            key => {
                if vk.is_some() {
                    return Err(format!("\"{text}\" has more than one key"));
                }
                vk = Some(
                    key_code(key).ok_or_else(|| format!("unknown key \"{part}\" in \"{text}\""))?,
                );
                continue;
            }
        }
        has_modifier = true;
    }
    let vk = vk.ok_or_else(|| format!("\"{text}\" has no key, only modifiers"))?;
    if !has_modifier {
        return Err(format!("\"{text}\" needs Ctrl, Alt, Shift or Win"));
    }
    Ok(Hotkey { modifiers, vk })
}

fn key_code(key: &str) -> Option<u32> {
    let bytes = key.as_bytes();
    if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
        return Some(bytes[0].to_ascii_uppercase() as u32);
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x70 + n - 1); // VK_F1..VK_F24
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_shortcuts() {
        let k = parse("Ctrl+Alt+G").unwrap();
        assert_eq!(k.vk, 'G' as u32);
        assert_eq!(k.modifiers, MOD_NOREPEAT | MOD_CONTROL | MOD_ALT);
        assert_eq!(parse("win + shift + f9").unwrap().vk, 0x78);
        assert_eq!(parse("Alt+1").unwrap().vk, '1' as u32);
    }

    #[test]
    fn rejects_unsafe_or_broken_shortcuts() {
        assert!(parse("G").is_err());
        assert!(parse("Ctrl+Alt").is_err());
        assert!(parse("Ctrl+G+H").is_err());
        assert!(parse("Ctrl+Space").is_err());
        assert!(parse("Ctrl+F25").is_err());
    }
}
