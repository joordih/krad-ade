use std::path::Path;
use std::time::SystemTime;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const TOO_LARGE_TEXT: &str = "This file is larger than 2 MB, too large to edit here.";
const NOT_TEXT_TEXT: &str = "This file is not UTF-8 text.";

pub(super) fn language_for(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let language = match extension.as_str() {
        "rs" => "rust",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "json" => "json",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "html" | "htm" => "html",
        "css" => "css",
        "py" => "python",
        "sh" | "bash" | "zsh" => "bash",
        "yaml" | "yml" => "yaml",
        _ => return None,
    };
    Some(language)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DiskChange {
    Unchanged,
    Reload,
    Conflict,
}

pub(super) fn disk_change(
    known: Option<SystemTime>,
    disk: Option<SystemTime>,
    dirty: bool,
) -> DiskChange {
    match disk {
        Some(disk) if Some(disk) != known && dirty => DiskChange::Conflict,
        Some(disk) if Some(disk) != known => DiskChange::Reload,
        _ => DiskChange::Unchanged,
    }
}

pub(super) struct ReadFile {
    pub(super) text: String,
    pub(super) modified: Option<SystemTime>,
}

#[derive(Debug)]
pub(super) enum ReadError {
    TooLarge,
    NotText,
    Io(String),
}

impl ReadError {
    pub(super) fn text(&self) -> &str {
        match self {
            Self::TooLarge => TOO_LARGE_TEXT,
            Self::NotText => NOT_TEXT_TEXT,
            Self::Io(reason) => reason,
        }
    }
}

pub(super) fn read_file(path: &Path) -> Result<ReadFile, ReadError> {
    let metadata = std::fs::metadata(path).map_err(|error| ReadError::Io(error.to_string()))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(ReadError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|error| ReadError::Io(error.to_string()))?;
    let text = String::from_utf8(bytes).map_err(|_| ReadError::NotText)?;
    Ok(ReadFile {
        text,
        modified: metadata.modified().ok(),
    })
}

pub(super) fn modified_at(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn languages_follow_the_extension() {
        assert_eq!(language_for(Path::new("src/main.rs")), Some("rust"));
        assert_eq!(language_for(Path::new("Cargo.TOML")), Some("toml"));
        assert_eq!(language_for(Path::new("app.tsx")), Some("tsx"));
        assert_eq!(language_for(Path::new("notes.txt")), None);
        assert_eq!(language_for(Path::new("Makefile")), None);
    }

    #[test]
    fn the_disk_reloads_a_clean_tab_and_asks_about_a_dirty_one() {
        let before = SystemTime::UNIX_EPOCH;
        let after = before + Duration::from_secs(1);
        assert_eq!(
            disk_change(Some(before), Some(before), true),
            DiskChange::Unchanged
        );
        assert_eq!(
            disk_change(Some(before), Some(after), false),
            DiskChange::Reload
        );
        assert_eq!(
            disk_change(Some(before), Some(after), true),
            DiskChange::Conflict
        );
        assert_eq!(disk_change(Some(before), None, true), DiskChange::Unchanged);
    }
}
