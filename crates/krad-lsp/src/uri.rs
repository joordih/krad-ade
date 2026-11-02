use lsp_types::Uri;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::str::FromStr;

use crate::error::LspError;

const FILE_SCHEME: &str = "file://";
const PERCENT: u8 = b'%';
const KEPT: &[u8] = b"-._~/:";
const HEX: &[u8; 16] = b"0123456789ABCDEF";
const RADIX: u32 = 16;
const WINDOWS_SEPARATOR: char = '\\';
const SEPARATOR: char = '/';

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || KEPT.contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push(char::from(PERCENT));
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
    out
}

fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == PERCENT {
            let pair = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(pair, RADIX).ok()?);
            index += 3;
        } else {
            out.push(byte);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

const VERBATIM_UNC: &str = "//?/UNC/";
const VERBATIM: &str = "//?/";
const UNC: &str = "//";
const DRIVE_MARK: u8 = b':';

fn host_and_path(unc: &str) -> (&str, String) {
    match unc.split_once(SEPARATOR) {
        Some((host, path)) => (host, format!("{SEPARATOR}{path}")),
        None => (unc, SEPARATOR.to_string()),
    }
}

fn uri_text(path: &str) -> String {
    let slashed = path.replace(WINDOWS_SEPARATOR, "/");
    let (host, rooted) = if let Some(unc) = slashed.strip_prefix(VERBATIM_UNC) {
        host_and_path(unc)
    } else if let Some(local) = slashed.strip_prefix(VERBATIM) {
        ("", format!("{SEPARATOR}{local}"))
    } else if let Some(unc) = slashed.strip_prefix(UNC) {
        host_and_path(unc)
    } else if slashed.starts_with(SEPARATOR) {
        ("", slashed.clone())
    } else {
        ("", format!("{SEPARATOR}{slashed}"))
    };
    format!("{FILE_SCHEME}{}{}", encode(host), encode(&rooted))
}

fn path_text(after_scheme: &str, separator: char) -> Option<String> {
    let decoded = decode(after_scheme)?;
    let Some(local) = decoded.strip_prefix(SEPARATOR) else {
        let (host, path) = decoded.split_once(SEPARATOR).unwrap_or((&decoded, ""));
        let path = path.replace(SEPARATOR, &separator.to_string());
        return Some(format!("{separator}{separator}{host}{separator}{path}"));
    };
    let drive = local.as_bytes().get(1) == Some(&DRIVE_MARK);
    Some(if drive {
        local.to_owned()
    } else {
        decoded.clone()
    })
}

/// # Errors
///
/// Returns [`LspError::NotAPath`] when the path cannot form a `file` URI.
pub fn file_uri(path: &Path) -> Result<Uri, LspError> {
    Uri::from_str(&uri_text(&path.to_string_lossy()))
        .map_err(|_| LspError::NotAPath(path.to_string_lossy().into_owned()))
}

#[must_use]
pub fn uri_path(uri: &Uri) -> Option<PathBuf> {
    let after_scheme = uri.as_str().strip_prefix(FILE_SCHEME)?;
    path_text(after_scheme, MAIN_SEPARATOR).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_round_trips_through_its_uri() -> Result<(), LspError> {
        let path = Path::new("/work/my api/src/ñ.rs");
        let uri = file_uri(path)?;
        assert_eq!(uri.as_str(), "file:///work/my%20api/src/%C3%B1.rs");
        assert_eq!(uri_path(&uri).as_deref(), Some(path));
        Ok(())
    }

    #[test]
    fn a_windows_drive_keeps_its_letter() -> Result<(), LspError> {
        let uri = Uri::from_str("file:///C:/work/main.rs")
            .map_err(|_| LspError::NotAPath(String::new()))?;
        assert_eq!(uri_path(&uri), Some(PathBuf::from("C:/work/main.rs")));
        Ok(())
    }

    #[test]
    fn windows_network_and_extended_paths_name_their_host() {
        let cases = [
            (
                r"\\wsl.localhost\Ubuntu\home\me\app.js",
                "file://wsl.localhost/Ubuntu/home/me/app.js",
            ),
            (r"\\server\share\a b.js", "file://server/share/a%20b.js"),
            (r"\\?\UNC\server\share\a.js", "file://server/share/a.js"),
            (r"\\?\C:\work\a.js", "file:///C:/work/a.js"),
            (r"C:\work\a.js", "file:///C:/work/a.js"),
            ("/work/a.js", "file:///work/a.js"),
        ];
        for (path, uri) in cases {
            assert_eq!(uri_text(path), uri, "{path}");
            assert!(Uri::from_str(uri).is_ok(), "{uri}");
        }
    }

    #[test]
    fn a_uri_with_a_host_reads_back_as_a_network_path() {
        assert_eq!(
            path_text("wsl.localhost/Ubuntu/app.js", '\\').as_deref(),
            Some(r"\\wsl.localhost\Ubuntu\app.js")
        );
        assert_eq!(
            path_text("/C:/work/a.js", '\\').as_deref(),
            Some("C:/work/a.js")
        );
        assert_eq!(path_text("/work/a.js", '/').as_deref(), Some("/work/a.js"));
    }

    #[test]
    fn only_file_uris_name_a_path() -> Result<(), LspError> {
        let uri =
            Uri::from_str("untitled:Untitled-1").map_err(|_| LspError::NotAPath(String::new()))?;
        assert_eq!(uri_path(&uri), None);
        Ok(())
    }
}
