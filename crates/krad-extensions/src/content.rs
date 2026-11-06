use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::ExtensionError;

pub const MAX_CONTENT_FILES: usize = 20_000;
pub const MAX_CONTENT_BYTES: u64 = 256 * 1024 * 1024;
const GIT_DIR: &str = ".git";
const CHUNK: usize = 64 * 1024;
const FILE_TAG: &[u8] = b"file\0";
const LINK_TAG: &[u8] = b"link\0";
const SEPARATOR: &[u8] = b"\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDigest(pub String);

struct Walk {
    files: Vec<(PathBuf, Entry)>,
    bytes: u64,
}

enum Entry {
    File,
    Link(PathBuf),
}

/// # Errors
///
/// Returns [`ExtensionError::TooLarge`] when the folder holds more files or bytes than an
/// extension may, or [`ExtensionError::Io`] when a file cannot be read.
pub fn content_digest(folder: &Path) -> Result<ContentDigest, ExtensionError> {
    let mut walk = Walk {
        files: Vec::new(),
        bytes: 0,
    };
    collect(folder, Path::new(""), &mut walk, true)?;
    walk.files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    let mut chunk = vec![0; CHUNK];
    for (relative, entry) in &walk.files {
        let name = relative.to_string_lossy();
        match entry {
            Entry::File => {
                hasher.update(FILE_TAG);
                hasher.update(name.as_bytes());
                hasher.update(SEPARATOR);
                let mut file = fs::File::open(folder.join(relative))?;
                loop {
                    let read = file.read(&mut chunk)?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&chunk[..read]);
                }
            }
            Entry::Link(target) => {
                hasher.update(LINK_TAG);
                hasher.update(name.as_bytes());
                hasher.update(SEPARATOR);
                hasher.update(target.to_string_lossy().as_bytes());
            }
        }
        hasher.update(SEPARATOR);
    }
    Ok(ContentDigest(format!("{:x}", hasher.finalize())))
}

fn collect(root: &Path, relative: &Path, walk: &mut Walk, top: bool) -> Result<(), ExtensionError> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let name = entry.file_name();
        if top && name == GIT_DIR {
            continue;
        }
        let path = relative.join(&name);
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            push(walk, path, Entry::Link(fs::read_link(entry.path())?), 0)?;
        } else if kind.is_dir() {
            collect(root, &path, walk, false)?;
        } else if kind.is_file() {
            push(walk, path, Entry::File, entry.metadata()?.len())?;
        }
    }
    Ok(())
}

fn push(walk: &mut Walk, path: PathBuf, entry: Entry, size: u64) -> Result<(), ExtensionError> {
    walk.bytes = walk.bytes.saturating_add(size);
    if walk.files.len() >= MAX_CONTENT_FILES || walk.bytes > MAX_CONTENT_BYTES {
        return Err(ExtensionError::TooLarge);
    }
    walk.files.push((path, entry));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn folder(name: &str) -> io::Result<PathBuf> {
        let folder =
            std::env::temp_dir().join(format!("krad-content-{name}-{}", std::process::id()));
        if folder.exists() {
            fs::remove_dir_all(&folder)?;
        }
        fs::create_dir_all(folder.join("dist"))?;
        fs::write(folder.join("extension.toml"), "id = \"x\"")?;
        fs::write(folder.join("dist/main.js"), "console.log(1)")?;
        Ok(folder)
    }

    #[test]
    fn any_changed_byte_of_the_program_changes_the_digest() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = folder("changed")?;
        let before = content_digest(&root)?;
        assert_eq!(content_digest(&root)?, before);
        fs::write(root.join("dist/main.js"), "console.log(2)")?;
        assert_ne!(content_digest(&root)?, before);
        Ok(())
    }

    #[test]
    fn a_new_file_or_a_renamed_one_changes_the_digest() -> Result<(), Box<dyn std::error::Error>> {
        let root = folder("added")?;
        let before = content_digest(&root)?;
        fs::write(root.join("dist/extra.js"), "")?;
        let added = content_digest(&root)?;
        assert_ne!(added, before);
        fs::rename(root.join("dist/extra.js"), root.join("dist/other.js"))?;
        assert_ne!(content_digest(&root)?, added);
        Ok(())
    }

    #[test]
    fn the_git_folder_is_left_out() -> Result<(), Box<dyn std::error::Error>> {
        let root = folder("git")?;
        let before = content_digest(&root)?;
        fs::create_dir_all(root.join(".git"))?;
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main")?;
        assert_eq!(content_digest(&root)?, before);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_hashed_by_its_target() -> Result<(), Box<dyn std::error::Error>> {
        let root = folder("link")?;
        std::os::unix::fs::symlink("/usr/bin/env", root.join("dist/tool"))?;
        let before = content_digest(&root)?;
        fs::remove_file(root.join("dist/tool"))?;
        std::os::unix::fs::symlink("/bin/sh", root.join("dist/tool"))?;
        assert_ne!(content_digest(&root)?, before);
        Ok(())
    }
}
