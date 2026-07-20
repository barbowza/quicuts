//! Host classes for hosted collections (ADR 0003): named groups of host
//! executables a hosted manifest can match via `Host: <class>`. Quicuts owns
//! the built-in lists centrally; users extend them in settings.

use std::collections::{HashMap, HashSet};

use crate::normalize_exe;

/// The only host class today: web apps living inside a browser.
pub const BROWSER_CLASS: &str = "browser";

/// Built-in browser exes, pre-normalized (lowercase, no ".exe").
pub const BUILTIN_BROWSER_EXES: &[&str] = &[
    "chrome", "msedge", "firefox", "brave", "opera", "opera_gx", "vivaldi",
    "chromium", "arc", "zen", "librewolf", "waterfox",
];

/// Host class name -> merged exe set (built-in plus user extensions).
#[derive(Debug, Clone)]
pub struct HostClasses {
    map: HashMap<String, HashSet<String>>,
}

impl HostClasses {
    pub fn builtin() -> Self {
        Self::with_extensions(&[])
    }

    /// Built-in classes extended with user-supplied browser exe names
    /// (any case, ".exe" optional, paths tolerated).
    pub fn with_extensions(extra_browser: &[String]) -> Self {
        let mut browsers: HashSet<String> =
            BUILTIN_BROWSER_EXES.iter().map(|e| e.to_string()).collect();
        browsers.extend(
            extra_browser
                .iter()
                .map(|e| normalize_exe(e))
                .filter(|e| !e.is_empty()),
        );
        let mut map = HashMap::new();
        map.insert(BROWSER_CLASS.to_string(), browsers);
        Self { map }
    }

    /// Is `exe` a member of `class`? `class` is compared lowercase; `exe`
    /// is normalized before lookup.
    pub fn contains(&self, class: &str, exe: &str) -> bool {
        self.map
            .get(&class.trim().to_ascii_lowercase())
            .is_some_and(|set| set.contains(&normalize_exe(exe)))
    }
}

impl Default for HostClasses {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_browsers() {
        let hc = HostClasses::builtin();
        assert!(hc.contains("browser", "chrome"));
        assert!(hc.contains("browser", "CHROME.EXE"));
        assert!(hc.contains("browser", "msedge.exe"));
        assert!(hc.contains("browser", "firefox"));
        assert!(!hc.contains("browser", "notepad.exe"));
        assert!(!hc.contains("terminal", "chrome"));
    }

    #[test]
    fn extensions_normalize() {
        let hc = HostClasses::with_extensions(&["Thorium.EXE".into(), "  ".into()]);
        assert!(hc.contains("browser", "thorium"));
        assert!(hc.contains("Browser", "thorium.exe"));
        assert!(!hc.contains("browser", ""));
    }
}
