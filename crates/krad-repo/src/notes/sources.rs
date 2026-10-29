use krad_domain::DOCS_SOURCES_FILE;
use std::fs;
use std::path::Path;

use super::{is_note, vault_dir, HIDDEN_PREFIX, PATH_SEPARATOR};
use crate::atomic::write_atomically;
use crate::RepoError;

const SOURCES_KEY: &str = "sources";
const SOURCES_OPEN: char = '[';
const SOURCES_CLOSE: char = ']';
const SOURCES_SEPARATOR: char = ',';
const QUOTE: char = '"';
const SKIPPED_SOURCES: [&str; 3] = ["target", "node_modules", "dist"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEntry {
    pub relative: String,
    pub is_dir: bool,
    pub children: usize,
}

/// # Errors
///
/// Returns [`RepoError::Io`] when the file exists but cannot be read.
pub fn read_docs_sources(root: &Path) -> Result<Option<Vec<String>>, RepoError> {
    let path = vault_dir(root).join(DOCS_SOURCES_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    let sources = text
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == SOURCES_KEY).then_some(value)
        })
        .map_or_default(|value| {
            value
                .trim()
                .trim_start_matches(SOURCES_OPEN)
                .trim_end_matches(SOURCES_CLOSE)
                .split(SOURCES_SEPARATOR)
                .map(|item| item.trim().trim_matches(QUOTE).to_owned())
                .filter(|item| !item.is_empty())
                .collect()
        });
    Ok(Some(sources))
}

/// # Errors
///
/// Returns [`RepoError::Io`] when the file cannot be written.
pub fn write_docs_sources(root: &Path, sources: &[String]) -> Result<(), RepoError> {
    let quoted: Vec<String> = sources
        .iter()
        .map(|source| format!("{QUOTE}{}{QUOTE}", source.replace(QUOTE, "")))
        .collect();
    write_atomically(
        &vault_dir(root).join(DOCS_SOURCES_FILE),
        &format!(
            "{SOURCES_KEY} = {SOURCES_OPEN}{}{SOURCES_CLOSE}\n",
            quoted.join(", ")
        ),
    )
}

/// # Errors
///
/// Returns [`RepoError::Io`] when the folder cannot be read.
pub fn docs_source_candidates(root: &Path) -> Result<Vec<SourceEntry>, RepoError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .map_or_default(|name| name.to_string_lossy().into_owned());
        if name.starts_with(HIDDEN_PREFIX) || SKIPPED_SOURCES.contains(&name.as_str()) {
            continue;
        }
        if path.is_dir() {
            let children = fs::read_dir(&path).map_or(0, Iterator::count);
            entries.push(SourceEntry {
                relative: format!("{name}{PATH_SEPARATOR}"),
                is_dir: true,
                children,
            });
        } else if is_note(&path) {
            entries.push(SourceEntry {
                relative: name,
                is_dir: false,
                children: 0,
            });
        }
    }
    entries.sort_by_cached_key(|entry| (!entry.is_dir, entry.relative.to_lowercase()));
    Ok(entries)
}
