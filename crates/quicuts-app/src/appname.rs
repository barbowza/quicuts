//! Friendly display name for an executable: the version-info
//! `FileDescription` ("Windows Calculator"), falling back to the cleaned
//! exe stem ("CalculatorApp"). Like `icons`, this reads on-disk file
//! metadata only — it never hooks or inspects running processes.

/// Best display name for a foreground exe.
pub fn display_name(exe_path: Option<&str>, exe_name: &str) -> String {
    exe_path
        .and_then(file_description)
        .unwrap_or_else(|| stem(exe_name))
}

/// Exe name minus any directory prefix and one trailing `.exe`.
fn stem(exe_name: &str) -> String {
    let base = exe_name.rsplit(['\\', '/']).next().unwrap_or(exe_name);
    match base.get(base.len().saturating_sub(4)..) {
        Some(ext) if ext.eq_ignore_ascii_case(".exe") => base[..base.len() - 4].to_string(),
        _ => base.to_string(),
    }
}

#[cfg(windows)]
fn file_description(exe_path: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let path = wide(exe_path);
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut block = vec![0u8; size as usize];
        GetFileVersionInfoW(
            PCWSTR(path.as_ptr()),
            None,
            size,
            block.as_mut_ptr() as *mut _,
        )
        .ok()?;

        let query = |sub: &str| -> Option<(*const core::ffi::c_void, u32)> {
            let sub = wide(sub);
            let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut len: u32 = 0;
            let ok = VerQueryValueW(
                block.as_ptr() as *const _,
                PCWSTR(sub.as_ptr()),
                &mut ptr,
                &mut len,
            );
            (ok.as_bool() && !ptr.is_null() && len > 0).then_some((ptr as *const _, len))
        };

        // Declared translations first, then the en-US/Unicode conventions
        // some exes use without declaring.
        let mut langs: Vec<(u16, u16)> = Vec::new();
        if let Some((ptr, len)) = query("\\VarFileInfo\\Translation") {
            let pairs = std::slice::from_raw_parts(ptr as *const u16, (len / 2) as usize);
            langs.extend(pairs.chunks_exact(2).map(|c| (c[0], c[1])));
        }
        langs.push((0x0409, 0x04B0));
        langs.push((0x0409, 0x04E4));

        for (lang, cp) in langs {
            let sub = format!("\\StringFileInfo\\{lang:04X}{cp:04X}\\FileDescription");
            if let Some((ptr, len)) = query(&sub) {
                // len counts u16s including the NUL; stop at the first NUL.
                let raw = std::slice::from_raw_parts(ptr as *const u16, len as usize);
                let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                let s = String::from_utf16_lossy(&raw[..end]).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
    }
}

#[cfg(not(windows))]
fn file_description(_exe_path: &str) -> Option<String> {
    None
}
