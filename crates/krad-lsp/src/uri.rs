use lsp_types::Uri;
use std::path::{Path, PathBuf};
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

/// # Errors
///
/// Returns [`LspError::NotAPath`] when the path cannot form a `file` URI.
pub fn file_uri(path: &Path) -> Result<Uri, LspError> {
    let text = path.to_string_lossy().replace(WINDOWS_SEPARATOR, "/");
    let rooted = if text.starts_with(SEPARATOR) {
        text
    } else {
        format!("{SEPARATOR}{text}")
    };
    Uri::from_str(&format!("{FILE_SCHEME}{}", encode(&rooted)))
        .map_err(|_| LspError::NotAPath(path.to_string_lossy().into_owned()))
}

#[must_use]
pub fn uri_path(uri: &Uri) -> Option<PathBuf> {
    let decoded = decode(uri.as_str().strip_prefix(FILE_SCHEME)?)?;
    let drive = decoded
        .strip_prefix(SEPARATOR)
        .filter(|rest| rest.as_bytes().get(1) == Some(&b':'));
    Some(PathBuf::from(drive.unwrap_or(&decoded)))
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
    fn only_file_uris_name_a_path() -> Result<(), LspError> {
        let uri =
            Uri::from_str("untitled:Untitled-1").map_err(|_| LspError::NotAPath(String::new()))?;
        assert_eq!(uri_path(&uri), None);
        Ok(())
    }
}
