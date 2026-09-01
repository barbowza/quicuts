//! Host classes for hosted collections (ADR 0003): named groups of host
//! executables a hosted manifest can match via `Host: <class>`. Quicuts owns
//! the built-in lists centrally; users extend them in settings.

use std::collections::{HashMap, HashSet};

use crate::normalize_exe;

/// The only host class today: web apps living inside a browser.
pub const BROWSER_CLASS: &str = "browser";

/// Built-in browser exes, pre-normalized (lowercase, no ".exe"). The
/// identity a Windows agent reports.
pub const BUILTIN_BROWSER_EXES: &[&str] = &[
    "chrome", "msedge", "firefox", "brave", "opera", "opera_gx", "vivaldi",
    "chromium", "arc", "zen", "librewolf", "waterfox",
];

/// Built-in browser **bundle identifiers**, pre-normalized (lowercase) —
/// the identity a macOS agent reports for the same browsers.
///
/// These live in the *same* class set as the exe stems rather than a
/// parallel per-platform map: `match_foreground` must stay free of platform
/// branches (see CLAUDE.md), and the two namespaces cannot collide (a
/// bundle id always contains dots, an exe stem never does). A cross-built
/// Windows binary carrying a dozen extra strings costs nothing, and keeping
/// them unconditional means `just test` on the Linux toolchain covers them.
pub const BUILTIN_BROWSER_BUNDLE_IDS: &[&str] = &[
    "com.apple.safari",
    "com.apple.safaritechnologypreview",
    "com.google.chrome",
    "com.google.chrome.beta",
    "com.google.chrome.canary",
    "com.microsoft.edgemac",
    "com.microsoft.edgemac.beta",
    "org.mozilla.firefox",
    "org.mozilla.firefoxdeveloperedition",
    "org.mozilla.nightly",
    "com.brave.browser",
    "com.brave.browser.beta",
    "com.operasoftware.opera",
    "com.operasoftware.operagx",
    "com.vivaldi.vivaldi",
    "org.chromium.chromium",
    "company.thebrowser.browser", // Arc
    "app.zen-browser.zen",
    "org.mozilla.librewolf",
    "net.waterfox.waterfox",
    "com.kagi.kagimacos", // Orion
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
        let mut browsers: HashSet<String> = BUILTIN_BROWSER_EXES
            .iter()
            .chain(BUILTIN_BROWSER_BUNDLE_IDS)
            .map(|e| e.to_string())
            .collect();
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

    /// The macOS agent reports bundle ids, so the browser class has to
    /// recognize them as well as Windows exe stems.
    #[test]
    fn builtin_browser_bundle_ids() {
        let hc = HostClasses::builtin();
        assert!(hc.contains("browser", "com.apple.Safari"));
        assert!(hc.contains("browser", "com.google.Chrome"));
        assert!(hc.contains("browser", "org.mozilla.firefox"));
        // Firefox Developer Edition is a distinct id, not a suffix of the
        // release one — substring matching would be wrong here, so assert
        // the exact id is listed in its own right.
        assert!(hc.contains("browser", "org.mozilla.firefoxdeveloperedition"));
        assert!(hc.contains("browser", "com.microsoft.edgemac"));
        // Bundle ids are matched whole: a mail client is not a browser just
        // because it shares a vendor prefix with one.
        assert!(!hc.contains("browser", "com.apple.mail"));
        assert!(!hc.contains("browser", "com.google"));
        assert!(!hc.contains("browser", "com.googlecode.iterm2"));
    }

    /// `normalize_exe` lowercases and strips one trailing ".exe"; neither
    /// must mangle the dots in a bundle id.
    #[test]
    fn bundle_ids_survive_normalization() {
        assert_eq!(normalize_exe("com.google.Chrome"), "com.google.chrome");
        let hc = HostClasses::builtin();
        assert!(hc.contains("BROWSER", "COM.GOOGLE.CHROME"));
    }

    #[test]
    fn extensions_normalize() {
        let hc = HostClasses::with_extensions(&["Thorium.EXE".into(), "  ".into()]);
        assert!(hc.contains("browser", "thorium"));
        assert!(hc.contains("Browser", "thorium.exe"));
        assert!(!hc.contains("browser", ""));
    }
}
