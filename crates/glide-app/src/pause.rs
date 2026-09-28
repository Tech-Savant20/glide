//! Pausing Glide while software that is wary of input tools runs: exam lockdown
//! and proctoring apps, and games with anti-cheat.
//!
//! Such software is right to be suspicious of anything that reads and injects
//! input, so while it runs Glide removes its mouse hook entirely rather than try
//! to coexist.

/// Exam lockdown and proctoring apps. Each is matched against the start of process
/// names with case, spaces, dashes and dots ignored, so `SafeExamBrowser.Client.exe`
/// matches `safeexambrowser`.
pub const EXAM_APPS: &[&str] = &[
    "safeexambrowser", // Safe Exam Browser
    "lockdownbrowser", // Respondus LockDown Browser (and the OEM edition)
    "examplify",       // ExamSoft Examplify
    "guardianbrowser", // ProctorU / Meazure Guardian Browser
    "onvue",           // Pearson VUE OnVUE
    "browserlock",     // Pearson VUE browser lock
    "inspera",         // Inspera Exam Portal / Integrity Browser
];

/// Games with kernel or service anti-cheat, and the anti-cheat services that run
/// only while such a game does. Riot Vanguard's own service runs all the time, so
/// its games are listed by name instead.
pub const ANTI_CHEAT_GAMES: &[&str] = &[
    "valorantwin64shipping", // VALORANT (Riot Vanguard)
    "leagueoflegends",       // League of Legends (Riot Vanguard)
    "easyanticheat",         // Easy Anti-Cheat, while an EAC game runs
    "beservice",             // BattlEye, while a BattlEye game runs
];

fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The first running process matching one of `built_in` or `extra` (user-added
/// names from the config), if any.
pub fn find_running<'a>(
    processes: &'a [String],
    built_in: &[&str],
    extra: &[String],
) -> Option<&'a str> {
    let patterns: Vec<String> = built_in
        .iter()
        .map(|p| normalize(p))
        .chain(extra.iter().map(|p| normalize(p.trim_end_matches(".exe"))))
        .filter(|p| !p.is_empty())
        .collect();
    // Prefix, not substring: "beservice" must not match "WindscribeService.exe".
    processes.iter().map(String::as_str).find(|name| {
        let name = normalize(name);
        patterns.iter().any(|p| name.starts_with(p.as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn finds_built_in_exam_apps_whatever_the_spelling() {
        for exe in [
            "SafeExamBrowser.Client.exe",
            "LockDownBrowserOEM.exe",
            "Guardian Browser.exe",
            "OnVUE.exe",
            "Inspera Exam Portal.exe",
        ] {
            let running = names(&["explorer.exe", exe]);
            assert_eq!(find_running(&running, EXAM_APPS, &[]), Some(exe));
        }
    }

    #[test]
    fn finds_anti_cheat_games_but_not_the_always_on_vanguard_service() {
        for exe in [
            "VALORANT-Win64-Shipping.exe",
            "League of Legends.exe",
            "EasyAntiCheat_EOS.exe",
            "BEService.exe",
        ] {
            let running = names(&["vgc.exe", exe]);
            assert_eq!(find_running(&running, ANTI_CHEAT_GAMES, &[]), Some(exe));
        }
        let idle = names(&["vgc.exe", "RiotClientServices.exe", "explorer.exe"]);
        assert_eq!(find_running(&idle, ANTI_CHEAT_GAMES, &[]), None);
    }

    #[test]
    fn ignores_ordinary_apps() {
        let running = names(&[
            "chrome.exe",
            "Code.exe",
            "Rainmeter.exe",
            "explorer.exe",
            "WindscribeService.exe",
            "SearchIndexer.exe",
        ]);
        assert_eq!(find_running(&running, EXAM_APPS, &[]), None);
        assert_eq!(find_running(&running, ANTI_CHEAT_GAMES, &[]), None);
    }

    #[test]
    fn user_added_apps_count_too() {
        let running = names(&["chrome.exe", "MyUni-Exam.exe"]);
        assert_eq!(
            find_running(&running, EXAM_APPS, &["myuni-exam.exe".into()]),
            Some("MyUni-Exam.exe")
        );
        assert_eq!(find_running(&running, EXAM_APPS, &["".into()]), None);
    }
}
