use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{ExtensionIndex, IndexRepository};
use crate::error::ExtensionError;
use crate::http;

const INDEX_DIR: &str = "extension-index";
const INDEX_FILE: &str = "index.json";
const PARTIAL_SUFFIX: &str = "partial";
const RAW_GITHUB: &str = "https://raw.githubusercontent.com";
const DEFAULT_BRANCH: &str = "HEAD";
pub const MAX_INDEX_AGE: Duration = Duration::from_hours(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    CachedIfFresh,
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexOrigin {
    Fetched,
    Cached,
    StaleCache,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedIndex {
    pub index: ExtensionIndex,
    pub origin: IndexOrigin,
    pub age: Duration,
}

fn cache_file(config_dir: &Path, repository: &IndexRepository) -> PathBuf {
    config_dir
        .join(INDEX_DIR)
        .join(repository.to_string())
        .join(INDEX_FILE)
}

fn index_url(repository: &IndexRepository) -> String {
    format!("{RAW_GITHUB}/{repository}/{DEFAULT_BRANCH}/{INDEX_FILE}")
}

fn cached(config_dir: &Path, repository: &IndexRepository) -> Option<(ExtensionIndex, Duration)> {
    let file = cache_file(config_dir, repository);
    let age = fs::metadata(&file)
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .unwrap_or_default();
    let text = fs::read_to_string(&file).ok()?;
    ExtensionIndex::parse(&text).ok().map(|index| (index, age))
}

fn store(config_dir: &Path, repository: &IndexRepository, text: &str) -> io::Result<()> {
    let file = cache_file(config_dir, repository);
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    let partial = file.with_extension(PARTIAL_SUFFIX);
    fs::write(&partial, text)?;
    fs::rename(partial, file)
}

pub(super) fn load_with(
    config_dir: &Path,
    repository: &IndexRepository,
    freshness: Freshness,
    fetch: impl FnOnce() -> Result<String, ExtensionError>,
) -> Result<LoadedIndex, ExtensionError> {
    let kept = cached(config_dir, repository);
    if let (Freshness::CachedIfFresh, Some((index, age))) = (freshness, &kept) {
        if *age < MAX_INDEX_AGE {
            return Ok(LoadedIndex {
                index: index.clone(),
                origin: IndexOrigin::Cached,
                age: *age,
            });
        }
    }
    let fetched = fetch().and_then(|text| {
        let index = ExtensionIndex::parse(&text).map_err(ExtensionError::from)?;
        store(config_dir, repository, &text)?;
        Ok(index)
    });
    match (fetched, kept) {
        (Ok(index), _) => Ok(LoadedIndex {
            index,
            origin: IndexOrigin::Fetched,
            age: Duration::ZERO,
        }),
        (Err(error), Some((index, age))) => {
            tracing::debug!(%error, "extension index fetch failed, using the cached copy");
            Ok(LoadedIndex {
                index,
                origin: IndexOrigin::StaleCache,
                age,
            })
        }
        (Err(error), None) => Err(error),
    }
}

/// # Errors
///
/// Returns [`ExtensionError`] when the index is neither cached nor reachable, or what was fetched
/// is not an index.
#[tracing::instrument(skip_all, fields(repository = %repository))]
pub fn load_index(
    config_dir: &Path,
    repository: &IndexRepository,
    freshness: Freshness,
) -> Result<LoadedIndex, ExtensionError> {
    load_with(config_dir, repository, freshness, || {
        http::fetch_text(&http::agent(), &index_url(repository))
    })
}

#[must_use]
pub fn cached_index(config_dir: &Path, repository: &IndexRepository) -> Option<LoadedIndex> {
    cached(config_dir, repository).map(|(index, age)| LoadedIndex {
        index,
        origin: IndexOrigin::Cached,
        age,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const INDEX: &str = r#"{"extensions":[{"id":"docker","name":"Docker","repository":"https://github.com/joordih/krad-docker","commit":"0123456789abcdef0123456789abcdef01234567","api":"1.0"}]}"#;

    fn config(name: &str) -> io::Result<PathBuf> {
        let dir = std::env::temp_dir().join(format!("krad-index-{name}-{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn a_fresh_cache_answers_without_fetching() -> TestResult {
        let dir = config("fresh")?;
        let first = load_with(
            &dir,
            &IndexRepository::default(),
            Freshness::CachedIfFresh,
            || Ok(INDEX.to_owned()),
        )?;
        assert_eq!(first.origin, IndexOrigin::Fetched);
        let second = load_with(
            &dir,
            &IndexRepository::default(),
            Freshness::CachedIfFresh,
            || Err(ExtensionError::Missing("network".to_owned())),
        )?;
        assert_eq!(second.origin, IndexOrigin::Cached);
        assert_eq!(second.index, first.index);
        Ok(())
    }

    #[test]
    fn a_failed_refresh_falls_back_to_the_cached_copy() -> TestResult {
        let dir = config("stale")?;
        load_with(
            &dir,
            &IndexRepository::default(),
            Freshness::Refresh,
            || Ok(INDEX.to_owned()),
        )?;
        let loaded = load_with(
            &dir,
            &IndexRepository::default(),
            Freshness::Refresh,
            || Err(ExtensionError::Missing("network".to_owned())),
        )?;
        assert_eq!(loaded.origin, IndexOrigin::StaleCache);
        assert_eq!(loaded.index.entries.len(), 1);
        Ok(())
    }

    #[test]
    fn nothing_cached_and_nothing_fetched_is_an_error() -> TestResult {
        let dir = config("empty")?;
        let loaded = load_with(
            &dir,
            &IndexRepository::default(),
            Freshness::CachedIfFresh,
            || Ok("not json".to_owned()),
        );
        assert!(loaded.is_err());
        assert!(cached_index(&dir, &IndexRepository::default()).is_none());
        Ok(())
    }

    #[test]
    fn each_index_repository_keeps_its_own_cache() -> TestResult {
        let dir = config("repositories")?;
        load_with(
            &dir,
            &IndexRepository::default(),
            Freshness::Refresh,
            || Ok(INDEX.to_owned()),
        )?;
        let other: IndexRepository = "acme/extensions".parse()?;
        assert!(cached_index(&dir, &other).is_none());
        assert!(cached_index(&dir, &IndexRepository::default()).is_some());
        Ok(())
    }
}
