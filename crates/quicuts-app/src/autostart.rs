//! macOS: keep the login item pointing at the running app.
//!
//! The autostart plugin's `is_enabled` only checks that
//! `~/Library/LaunchAgents/<name>.plist` exists, not which binary it starts.
//! Turning Launch at login on from `just mac-run` records
//! `target/debug/quicuts`, which launchd then starts as its own responsible
//! process, with no Accessibility grant: every login brought up an agent
//! stuck on `PermissionRequired`, and launching the installed app never
//! corrected the pointer (2026-10-17).

use std::path::{Path, PathBuf};

use tauri_plugin_autostart::ManagerExt;

/// An installed (or `mac-build`) copy, as opposed to a bare dev binary.
pub fn is_bundled(exe: &Path) -> bool {
    exe.to_string_lossy().contains(".app/Contents/MacOS/")
}

/// Rewrite the login item when it starts some other binary. Only a bundled
/// copy repoints it: a dev run must not take login over from the installed app.
pub fn repoint_if_stale(app: &tauri::AppHandle) {
    let Ok(current) = std::env::current_exe().and_then(|p| p.canonicalize()) else {
        return;
    };
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let plist = Path::new(&home)
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", app.package_info().name));
    let registered = registered_program(&plist).map(|p| p.canonicalize().unwrap_or(p));
    if !should_repoint(registered.as_deref(), &current) {
        return;
    }
    let mgr = app.autolaunch();
    match mgr.disable().and_then(|_| mgr.enable()) {
        Ok(()) => log::info!(
            "autostart: login item repointed from {} to {}",
            registered.map_or("(unreadable)".into(), |p| p.display().to_string()),
            current.display()
        ),
        Err(e) => log::warn!("autostart: couldn't repoint the login item: {e}"),
    }
}

/// The binary a LaunchAgent plist starts: `ProgramArguments[0]`.
fn registered_program(plist: &Path) -> Option<PathBuf> {
    let value = plist::Value::from_file(plist).ok()?;
    let args = value.as_dictionary()?.get("ProgramArguments")?.as_array()?;
    args.first()?.as_string().map(PathBuf::from)
}

fn should_repoint(registered: Option<&Path>, current: &Path) -> bool {
    is_bundled(current) && registered != Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTALLED: &str = "/Applications/Quicuts.app/Contents/MacOS/quicuts";
    const DEV: &str = "/Users/me/quicuts/target/debug/quicuts";

    #[test]
    fn installed_app_takes_over_a_dev_pointer() {
        assert!(should_repoint(Some(Path::new(DEV)), Path::new(INSTALLED)));
    }

    #[test]
    fn installed_app_rewrites_an_unreadable_plist() {
        assert!(should_repoint(None, Path::new(INSTALLED)));
    }

    #[test]
    fn a_correct_pointer_is_left_alone() {
        assert!(!should_repoint(
            Some(Path::new(INSTALLED)),
            Path::new(INSTALLED)
        ));
    }

    #[test]
    fn a_dev_run_never_repoints() {
        assert!(!should_repoint(Some(Path::new(INSTALLED)), Path::new(DEV)));
        assert!(!should_repoint(None, Path::new(DEV)));
    }

    #[test]
    fn reads_program_arguments_from_the_plugins_plist() {
        let dir = std::env::temp_dir().join(format!("quicuts-autostart-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Quicuts.plist");
        // The shape tauri-plugin-autostart's LaunchAgent mode writes.
        std::fs::write(
            &path,
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
  <key>Label</key>
  <string>Quicuts</string>
  <key>ProgramArguments</key>
  <array><string>{DEV}</string></array>
  <key>RunAtLoad</key>
  <true/>
  </dict>
</plist>"#
            ),
        )
        .unwrap();
        assert_eq!(registered_program(&path), Some(PathBuf::from(DEV)));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
