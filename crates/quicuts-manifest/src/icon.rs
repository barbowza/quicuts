//! Manifest `Icon:` files rendered as `data:` URIs. Platform-free — the
//! exe-icon extraction lives in the app crate; this only reads image files
//! that ship next to a manifest.

use std::path::Path;

/// Largest manifest icon file we'll inline as a data URI.
pub const MAX_ICON_BYTES: u64 = 512 * 1024;

/// Read an image file (a manifest's `Icon:`) and return it as a `data:`
/// URI. Mime is derived from the extension; unknown extensions and
/// oversized files return `None`.
pub fn file_data_uri(path: &Path) -> Option<String> {
    let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        _ => return None,
    };
    if std::fs::metadata(path).ok()?.len() > MAX_ICON_BYTES {
        log::warn!("manifest icon {} exceeds {MAX_ICON_BYTES} bytes, skipping", path.display());
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

/// Minimal base64 (standard alphabet, padded); avoids pulling a crate for a
/// handful of tiny icons. Also used by the app crate for exe icons.
pub fn base64_encode(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { A[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { A[(n & 63) as usize] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn file_data_uri_round_trip() {
        let dir = std::env::temp_dir().join(format!("quicuts-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.png");
        std::fs::write(&p, b"fake-png-bytes").unwrap();
        let uri = file_data_uri(&p).unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
        assert!(uri.ends_with(&base64_encode(b"fake-png-bytes")));

        // Unknown extension and missing file both yield None.
        let bad = dir.join("x.exe");
        std::fs::write(&bad, b"nope").unwrap();
        assert_eq!(file_data_uri(&bad), None);
        assert_eq!(file_data_uri(&dir.join("absent.png")), None);

        // Oversized files are refused.
        let big = dir.join("big.png");
        std::fs::write(&big, vec![0u8; (MAX_ICON_BYTES + 1) as usize]).unwrap();
        assert_eq!(file_data_uri(&big), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
