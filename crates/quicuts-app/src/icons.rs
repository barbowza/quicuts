//! Executable icon extraction, returned as a `data:` URI so no custom URI
//! scheme (and its 404 edge cases) is needed. Windows-only; other platforms
//! return `None` and the rail falls back to a glyph.
//!
//! Not AV-sensitive: this reads the icon resource of an on-disk exe, it does
//! not hook or inspect running processes.

#[cfg(windows)]
pub fn icon_data_uri(exe_path: &str) -> Option<String> {
    windows_impl::extract(exe_path).map(|png| {
        format!("data:image/png;base64,{}", base64_encode(&png))
    })
}

#[cfg(not(windows))]
pub fn icon_data_uri(_exe_path: &str) -> Option<String> {
    None
}

#[cfg(windows)]
mod windows_impl {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::Shell::SHDefExtractIconW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn extract(exe_path: &str) -> Option<Vec<u8>> {
        unsafe {
            let path = wide(exe_path);
            let mut hicon = HICON::default();
            // niconsize: LOWORD = large icon size.
            let hr = SHDefExtractIconW(
                PCWSTR(path.as_ptr()),
                0,
                0,
                Some(&mut hicon),
                None,
                32,
            );
            if hr.is_err() || hicon.is_invalid() {
                return None;
            }
            let result = icon_to_png(hicon);
            let _ = DestroyIcon(hicon);
            result
        }
    }

    unsafe fn icon_to_png(hicon: HICON) -> Option<Vec<u8>> {
        let mut info = ICONINFO::default();
        if GetIconInfo(hicon, &mut info).is_err() {
            return None;
        }
        let color = info.hbmColor;
        let mask = info.hbmMask;
        let cleanup = || {
            if !color.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(color.0));
            }
            if !mask.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(mask.0));
            }
        };

        let mut bmp = BITMAP::default();
        let got = GetObjectW(
            HGDIOBJ(color.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut _),
        );
        if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
            cleanup();
            return None;
        }
        let w = bmp.bmWidth;
        let h = bmp.bmHeight;

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let hdc = GetDC(Some(HWND::default()));
        let scan = GetDIBits(
            hdc,
            color,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(Some(HWND::default()), hdc);
        cleanup();
        if scan == 0 {
            return None;
        }

        // BGRA -> RGBA.
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        encode_png(w as u32, h as u32, &buf)
    }

    fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().ok()?;
            writer.write_image_data(rgba).ok()?;
        }
        Some(out)
    }
}

/// Manifest icon files (platform-free) live in the manifest crate; exe
/// icons share its base64.
pub use quicuts_manifest::icon::{base64_encode, file_data_uri};
