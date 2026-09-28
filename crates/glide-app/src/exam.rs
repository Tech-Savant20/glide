//! Pausing Glide while exam lockdown or proctoring software runs.
//!
//! Such software is right to be wary of anything that reads and injects input, so
//! while it runs Glide removes its mouse hook entirely rather than try to coexist.

/// Built-in exam apps, matched against process names with case, spaces, dashes and
/// dots ignored, so `SafeExamBrowser.Client.exe` matches `safeexambrowser`.
pub const BUILT_IN: &[&str] = &[
    "safeexambrowser", // Safe Exam Browser
    "lockdownbrowser", // Respondus LockDown Browser (and the OEM edition)
    "examplify",       // ExamSoft Examplify
    "guardianbrowser", // ProctorU / Meazure Guardian Browser
    "onvue",           // Pearson VUE OnVUE
    "browserlock",     // Pearson VUE browser lock
    "inspera",         // Inspera Exam Portal / Integrity Browser
];

fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The first running process that looks like an exam app, if any. `extra` holds
/// user-added names from the config.
pub fn find_running<'a>(
    processes: &'a [String],
    extra: &[String],
) -> Option<&'a str> {
    let patterns: Vec<String> = BUILT_IN
        .iter()
        .map(|p| normalize(p))
        .chain(extra.iter().map(|p| normalize(p.trim_end_matches(".exe"))))
        .filter(|p| !p.is_empty())
        .collect();
    processes.iter().map(String::as_str).find(|name| {
        let name = normalize(name);
        patterns.iter().any(|p| name.contains(p.as_str()))
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
            assert_eq!(find_running(&running, &[]), Some(exe));
        }
    }

    #[test]
    fn ignores_ordinary_apps() {
        let running = names(&["chrome.exe", "Code.exe", "Rainmeter.exe", "explorer.exe"]);
        assert_eq!(find_running(&running, &[]), None);
    }

    #[test]
    fn user_added_apps_count_too() {
        let running = names(&["chrome.exe", "MyUni-Exam.exe"]);
        assert_eq!(
            find_running(&running, &["myuni-exam.exe".into()]),
            Some("MyUni-Exam.exe")
        );
        assert_eq!(find_running(&running, &["".into()]), None);
    }
}
