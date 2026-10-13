//! Running-process snapshot, used to include `BackgroundProcess: true`
//! manifests (PowerToys, Telegram, ...) in the rail while their app runs.
//! This is plain process enumeration — no hooks or input access, which is
//! why it can live in the main app instead of the agent.
//!
//! Keys are whatever identity the agent reports, normalized the way
//! `ManifestStore::match_foreground` normalizes a `WindowFilter`: the exe
//! name on Windows, the bundle identifier on macOS.

#[cfg(windows)]
pub use win::{exe_path, running};

#[cfg(windows)]
mod win {
    use std::collections::HashMap;

    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// Snapshot of live processes: normalized exe name (lowercase, no
    /// `.exe`) -> pid of one such process. Keys match what
    /// `ManifestStore::match_foreground` passes to its running check.
    pub fn running() -> HashMap<String, u32> {
        let mut map = HashMap::new();
        unsafe {
            let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                Ok(s) => s,
                Err(_) => return map,
            };
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    let len = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                    map.entry(quicuts_manifest::normalize_exe(&name))
                        .or_insert(entry.th32ProcessID);
                    if Process32NextW(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
        map
    }

    /// Full image path for a pid (icon extraction); None if inaccessible.
    pub fn exe_path(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; MAX_PATH as usize];
            let mut len = buf.len() as u32;
            let path = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
            .ok()
            .map(|_| String::from_utf16_lossy(&buf[..len as usize]));
            let _ = CloseHandle(handle);
            path
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::{exe_path, running};

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::HashMap;

    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSRunningApplication, NSWorkspace};

    /// Running apps with a bundle identifier: normalized bundle id -> pid.
    /// Bundle-less processes (daemons, bare binaries) cannot match a mac
    /// manifest's `WindowFilter`, so they are skipped. `runningApplications`
    /// is a snapshot refreshed by the main run loop, and safe to read from
    /// any thread.
    pub fn running() -> HashMap<String, u32> {
        autoreleasepool(|_| {
            let mut map = HashMap::new();
            for app in NSWorkspace::sharedWorkspace().runningApplications().iter() {
                if app.isTerminated() {
                    continue;
                }
                if let Some(id) = app.bundleIdentifier() {
                    map.entry(quicuts_manifest::normalize_exe(&id.to_string()))
                        .or_insert(app.processIdentifier() as u32);
                }
            }
            map
        })
    }

    /// The app's `.app` bundle path (what `icons::icon_data_uri` reads),
    /// falling back to the executable for bundle-less processes — the same
    /// choice the mac agent makes for `ForegroundInfo.exe_path`.
    pub fn exe_path(pid: u32) -> Option<String> {
        autoreleasepool(|_| {
            let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)?;
            let url = app.bundleURL().or_else(|| app.executableURL())?;
            Some(url.path()?.to_string())
        })
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn running() -> std::collections::HashMap<String, u32> {
    Default::default()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn exe_path(_pid: u32) -> Option<String> {
    None
}

#[cfg(all(test, target_os = "macos"))]
mod mac_tests {
    #[test]
    fn running_apps_are_keyed_by_bundle_id_with_a_bundle_path() {
        // Finder always runs in a logged-in session (the Dock relaunches it).
        let running = super::running();
        let pid = *running.get("com.apple.finder").expect("Finder is running");
        assert_eq!(
            super::exe_path(pid).as_deref(),
            Some("/System/Library/CoreServices/Finder.app")
        );
    }
}
