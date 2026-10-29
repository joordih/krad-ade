use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::process::wsl_resolve_symlinks;
use crate::RepoError;

const GIT_DIR_NAME: &str = ".git";
const BINARY_SNIFF_BYTES: usize = 8 * 1024;
const NUL: u8 = 0;
const NEWLINE: u8 = b'\n';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
}

/// # Errors
pub fn list_dir(dir: &Path) -> Result<Vec<DirEntry>, RepoError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == GIT_DIR_NAME {
            continue;
        }
        let path = entry.path();
        let kind = entry_kind(&path)?;
        entries.push(DirEntry { name, path, kind });
    }
    sort_entries(&mut entries);
    Ok(entries)
}

pub fn sort_entries(entries: &mut [DirEntry]) {
    entries.sort_by_cached_key(|entry| (entry.kind == EntryKind::File, entry.name.to_lowercase()));
}

fn entry_kind(path: &Path) -> Result<EntryKind, RepoError> {
    let metadata = fs::metadata(path).or_else(|_| fs::symlink_metadata(path))?;
    Ok(if metadata.is_dir() {
        EntryKind::Directory
    } else {
        EntryKind::File
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFile {
    pub text: String,
    pub truncated: bool,
}

/// # Errors
pub fn read_text(path: &Path, max_bytes: u64) -> Result<TextFile, RepoError> {
    let mut bytes = Vec::new();
    open_following_wsl_symlinks(path)?
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    decode_text(bytes, max_bytes)
}

/// # Errors
///
/// Returns [`RepoError::Binary`] when `bytes` are not text.
pub fn decode_text(mut bytes: Vec<u8>, max_bytes: u64) -> Result<TextFile, RepoError> {
    let sniffed = &bytes[..bytes.len().min(BINARY_SNIFF_BYTES)];
    if sniffed.contains(&NUL) {
        return Err(RepoError::Binary);
    }
    let truncated = u64::try_from(bytes.len()).map_or(true, |len| len > max_bytes);
    if truncated {
        bytes.truncate(usize::try_from(max_bytes).unwrap_or(usize::MAX));
        if let Some(last_newline) = bytes.iter().rposition(|&byte| byte == NEWLINE) {
            bytes.truncate(last_newline + 1);
        }
    }
    Ok(TextFile {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
    })
}

fn open_following_wsl_symlinks(path: &Path) -> Result<File, RepoError> {
    match File::open(path) {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match wsl_resolve_symlinks(path)? {
                Some(resolved) => Ok(File::open(resolved)?),
                None => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> Result<PathBuf, Box<dyn Error>> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let dir =
            std::env::temp_dir().join(format!("krad-repo-{label}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn list_dir_puts_directories_first_sorted_and_skips_git() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("list")?;
        for folder in [".git", "src", "Assets"] {
            fs::create_dir(dir.join(folder))?;
        }
        for file in ["b.txt", "A.md", "c.rs"] {
            fs::write(dir.join(file), "")?;
        }

        let listed = list_dir(&dir);
        fs::remove_dir_all(&dir)?;

        let names: Vec<(String, EntryKind)> = listed?
            .into_iter()
            .map(|entry| (entry.name, entry.kind))
            .collect();
        let expected = [
            ("Assets", EntryKind::Directory),
            ("src", EntryKind::Directory),
            ("A.md", EntryKind::File),
            ("b.txt", EntryKind::File),
            ("c.rs", EntryKind::File),
        ]
        .map(|(name, kind)| (name.to_owned(), kind));
        assert_eq!(names, expected);
        Ok(())
    }

    #[test]
    fn read_text_truncates_at_last_complete_line() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("truncate")?;
        let path = dir.join("lines.txt");
        fs::write(&path, "one\ntwo\nthree\n")?;

        let partial = read_text(&path, 10);
        let whole = read_text(&path, 100);
        fs::remove_dir_all(&dir)?;

        assert_eq!(
            partial?,
            TextFile {
                text: "one\ntwo\n".to_owned(),
                truncated: true
            }
        );
        assert_eq!(
            whole?,
            TextFile {
                text: "one\ntwo\nthree\n".to_owned(),
                truncated: false
            }
        );
        Ok(())
    }

    #[test]
    fn read_text_rejects_binary_files() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("binary")?;
        let path = dir.join("image.bin");
        fs::write(&path, [0x89, b'P', b'N', b'G', 0x00, 0x01])?;

        let result = read_text(&path, 100);
        fs::remove_dir_all(&dir)?;

        assert!(matches!(result, Err(RepoError::Binary)));
        Ok(())
    }
}
