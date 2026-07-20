//! Running-process snapshot, used to include `BackgroundProcess: true`
//! manifests (PowerToys, Telegram, ...) in the rail while their app runs.
//! This is plain process enumeration — no hooks or input access, which is
//! why it can live in the main app instead of the agent.

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

#[cfg(not(windows))]
pub fn running() -> std::collections::HashMap<String, u32> {
    Default::default()
}

#[cfg(not(windows))]
pub fn exe_path(_pid: u32) -> Option<String> {
    None
}
