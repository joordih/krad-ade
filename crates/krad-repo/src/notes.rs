use krad_domain::{Note, ATTACHMENTS_DIR, DOCS_SOURCES_FILE, NOTE_EXTENSION, VAULT_DIR};
use std::fs;
use std::path::{Component, Path, PathBuf};

mod sources;

pub use sources::{docs_source_candidates, read_docs_sources, write_docs_sources, SourceEntry};

use crate::atomic::write_atomically;
use crate::RepoError;

const HIDDEN_PREFIX: char = '.';
const PATH_SEPARATOR: &str = "/";
const EXTENSION_DOT: char = '.';
const COPY_SEPARATOR: &str = " ";
const MAX_COPIES: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultEntryKind {
    Folder,
    Note,
    Attachment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultEntry {
    pub path: String,
    pub kind: VaultEntryKind,
}

#[must_use]
pub fn vault_dir(root: &Path) -> PathBuf {
    root.join(VAULT_DIR)
}

#[must_use]
pub fn vault_path(root: &Path, path: &str) -> Option<PathBuf> {
    let mut full = vault_dir(root);
    for part in Path::new(path).components() {
        match part {
            Component::Normal(part) => full.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(full)
}

fn inside(root: &Path, path: &str) -> Result<PathBuf, RepoError> {
    vault_path(root, path).ok_or_else(|| RepoError::OutsideRepository(path.to_owned()))
}

fn relative(vault: &Path, path: &Path) -> String {
    path.strip_prefix(vault)
        .unwrap_or(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(PATH_SEPARATOR)
}

fn is_note(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(NOTE_EXTENSION))
}

fn collect(vault: &Path, dir: &Path, entries: &mut Vec<VaultEntry>) -> Result<(), RepoError> {
    let mut children: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| !name.to_string_lossy().starts_with(HIDDEN_PREFIX))
        })
        .collect();
    children.sort_by_key(|path| (!path.is_dir(), path.to_string_lossy().to_lowercase()));
    for path in children {
        let kind = if path.is_dir() {
            VaultEntryKind::Folder
        } else if is_note(&path) {
            VaultEntryKind::Note
        } else {
            VaultEntryKind::Attachment
        };
        entries.push(VaultEntry {
            path: relative(vault, &path),
            kind,
        });
        if kind == VaultEntryKind::Folder {
            collect(vault, &path, entries)?;
        }
    }
    Ok(())
}

/// # Errors
///
/// Returns [`RepoError::Io`] when a folder cannot be read.
pub fn vault_entries(root: &Path) -> Result<Vec<VaultEntry>, RepoError> {
    let vault = vault_dir(root);
    let mut entries = Vec::new();
    if vault.is_dir() {
        collect(&vault, &vault, &mut entries)?;
    }
    entries.retain(|entry| entry.path != DOCS_SOURCES_FILE);
    Ok(entries)
}

/// # Errors
///
/// Returns [`RepoError::Io`] when the vault cannot be read; a note that
/// cannot be read is left out.
pub fn vault_notes(root: &Path) -> Result<Vec<(String, Note)>, RepoError> {
    let notes = vault_entries(root)?
        .into_iter()
        .filter(|entry| entry.kind == VaultEntryKind::Note)
        .filter_map(|entry| {
            read_note(root, &entry.path)
                .ok()
                .map(|note| (entry.path, note))
        })
        .collect();
    Ok(notes)
}

/// # Errors
///
/// Returns [`RepoError::OutsideRepository`] for a path that climbs out of
/// the vault and [`RepoError::Io`] when the note cannot be read.
pub fn read_note(root: &Path, path: &str) -> Result<Note, RepoError> {
    Ok(Note::parse(&fs::read_to_string(inside(root, path)?)?))
}

/// # Errors
///
/// Returns [`RepoError::OutsideRepository`] for a path that climbs out of
/// the vault and [`RepoError::Io`] when it cannot be written.
pub fn write_note(root: &Path, path: &str, note: &Note) -> Result<(), RepoError> {
    write_atomically(&inside(root, path)?, &note.to_markdown())
}

/// # Errors
///
/// Returns [`RepoError::OutsideRepository`] for a path that climbs out of
/// the vault and [`RepoError::Io`] when it cannot be written.
pub fn write_note_text(root: &Path, path: &str, text: &str) -> Result<(), RepoError> {
    write_atomically(&inside(root, path)?, text)
}

fn free_name(folder: &Path, stem: &str, extension: Option<&str>) -> PathBuf {
    let file = |suffix: Option<usize>| {
        let stem = suffix.map_or_else(|| stem.to_owned(), |n| format!("{stem}{COPY_SEPARATOR}{n}"));
        match extension {
            Some(extension) => format!("{stem}{EXTENSION_DOT}{extension}"),
            None => stem,
        }
    };
    std::iter::once(None)
        .chain((2..=MAX_COPIES).map(Some))
        .map(|suffix| folder.join(file(suffix)))
        .find(|path| !path.exists())
        .unwrap_or_else(|| folder.join(file(None)))
}

/// # Errors
///
/// Returns [`RepoError::OutsideRepository`] for a folder outside the vault
/// and [`RepoError::Io`] when the note cannot be written.
pub fn create_note(root: &Path, folder: &str, title: &str) -> Result<String, RepoError> {
    let dir = inside(root, folder)?;
    fs::create_dir_all(&dir)?;
    let path = free_name(&dir, title, Some(NOTE_EXTENSION));
    let stem = path.file_stem().map_or_else(
        || title.to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    write_atomically(&path, &format!("# {stem}\n\n"))?;
    Ok(relative(&vault_dir(root), &path))
}

/// # Errors
///
/// Returns [`RepoError::OutsideRepository`] for a folder outside the vault
/// and [`RepoError::Io`] when it cannot be created.
pub fn create_vault_folder(root: &Path, folder: &str, name: &str) -> Result<String, RepoError> {
    let dir = inside(root, folder)?;
    let path = free_name(&dir, name, None);
    fs::create_dir_all(&path)?;
    Ok(relative(&vault_dir(root), &path))
}

/// # Errors
///
/// Returns [`RepoError::InvalidName`] for a file with no name and
/// [`RepoError::Io`] when it cannot be copied.
pub fn add_attachment(root: &Path, file: &Path) -> Result<String, RepoError> {
    let stem = file
        .file_stem()
        .ok_or_else(|| RepoError::InvalidName(file.display().to_string()))?
        .to_string_lossy()
        .into_owned();
    let extension = file
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned());
    let dir = vault_dir(root).join(ATTACHMENTS_DIR);
    fs::create_dir_all(&dir)?;
    let target = free_name(&dir, &stem, extension.as_deref());
    fs::copy(file, &target)?;
    Ok(relative(&vault_dir(root), &target))
}

/// # Errors
///
/// Returns [`RepoError::Io`] when it cannot be written.
pub fn save_attachment(
    root: &Path,
    stem: &str,
    extension: &str,
    bytes: &[u8],
) -> Result<String, RepoError> {
    let dir = vault_dir(root).join(ATTACHMENTS_DIR);
    fs::create_dir_all(&dir)?;
    let target = free_name(&dir, stem, Some(extension));
    fs::write(&target, bytes)?;
    Ok(relative(&vault_dir(root), &target))
}

/// # Errors
///
/// Returns [`RepoError::Io`] when the vault cannot be read or a note
/// written.
pub fn stamp_new_notes(root: &Path, now: u64) -> Result<usize, RepoError> {
    let mut stamped = 0;
    for (path, mut note) in vault_notes(root)? {
        if note.needs_stamp() {
            note.stamp(now);
            write_note(root, &path, &note)?;
            stamped += 1;
        }
    }
    Ok(stamped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> Result<PathBuf, RepoError> {
        let root = std::env::temp_dir().join(format!("krad-notes-{name}-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    #[test]
    fn notes_folders_and_attachments_are_listed_created_and_read() -> Result<(), RepoError> {
        let root = temp_root("vault")?;
        let plan = create_note(&root, "", "Plan")?;
        let again = create_note(&root, "", "Plan")?;
        let folder = create_vault_folder(&root, "", "Ideas")?;
        let nested = create_note(&root, &folder, "Spark")?;
        let image = save_attachment(&root, "Pasted image", "png", &[1, 2, 3])?;
        assert_eq!((plan.as_str(), again.as_str()), ("Plan.md", "Plan 2.md"));
        assert_eq!(nested, "Ideas/Spark.md");
        assert_eq!(image, "attachments/Pasted image.png");
        let listed: Vec<(String, VaultEntryKind)> = vault_entries(&root)?
            .into_iter()
            .map(|entry| (entry.path, entry.kind))
            .collect();
        assert_eq!(
            listed,
            [
                ("attachments".to_owned(), VaultEntryKind::Folder),
                (
                    "attachments/Pasted image.png".to_owned(),
                    VaultEntryKind::Attachment
                ),
                ("Ideas".to_owned(), VaultEntryKind::Folder),
                ("Ideas/Spark.md".to_owned(), VaultEntryKind::Note),
                ("Plan 2.md".to_owned(), VaultEntryKind::Note),
                ("Plan.md".to_owned(), VaultEntryKind::Note),
            ]
        );
        assert_eq!(read_note(&root, &plan)?.title(""), "Plan");
        assert!(read_note(&root, "../escape.md").is_err());
        assert_eq!(vault_notes(&root)?.len(), 3);
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn docs_sources_round_trip() -> Result<(), RepoError> {
        let root = temp_root("sources")?;
        assert_eq!(read_docs_sources(&root)?, None);
        write_docs_sources(&root, &["crates".to_owned(), "AGENTS.md".to_owned()])?;
        assert_eq!(
            read_docs_sources(&root)?,
            Some(vec!["crates".to_owned(), "AGENTS.md".to_owned()])
        );
        assert!(vault_entries(&root)?.is_empty(), "sources.toml is no note");
        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
