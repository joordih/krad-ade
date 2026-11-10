use krad_domain::{
    COMPASS_FILE, KRADDIE_FILE_EXTENSION, LEGACY_COMPASS_FILE, LEGACY_KRADDIE_FILE_EXTENSION,
};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub(super) const KRADDIES_DIR: &str = "kraddies";
const LEGACY_KRADDIES_DIR: &str = "pets";
const LEGACY_DEFAULT_COMPASS: &str = "# Soul\n\n## Standing instructions\n\nWrite here what this pet should always keep in mind, on every run.";

pub(super) fn settle(folder: &Path) -> Vec<std::io::Error> {
    let mut problems = Vec::new();
    if let Some(legacy) = legacy_sibling(folder) {
        problems.extend(move_folder(&legacy, folder).err());
        problems.extend(rename_files(&legacy));
    }
    problems.extend(rename_files(folder));
    problems
}

pub(super) fn leftover(folder: &Path) -> Option<PathBuf> {
    legacy_sibling(folder).filter(|legacy| legacy.is_dir())
}

fn legacy_sibling(folder: &Path) -> Option<PathBuf> {
    (folder.file_name()? == KRADDIES_DIR).then(|| folder.with_file_name(LEGACY_KRADDIES_DIR))
}

fn move_folder(legacy: &Path, folder: &Path) -> std::io::Result<()> {
    if !legacy.is_dir() {
        return Ok(());
    }
    if !folder.exists() {
        return std::fs::rename(legacy, folder);
    }
    for entry in std::fs::read_dir(legacy)? {
        let entry = entry?;
        let target = folder.join(entry.file_name());
        if !target.exists() {
            std::fs::rename(entry.path(), target)?;
        }
    }
    match std::fs::remove_dir(legacy) {
        Err(error)
            if !matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Err(error)
        }
        _ => Ok(()),
    }
}

fn rename_files(folder: &Path) -> Vec<std::io::Error> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| {
            if path.is_dir() {
                settle_compass(&path).err()
            } else {
                settle_extension(&path).err()
            }
        })
        .collect()
}

fn settle_extension(path: &Path) -> std::io::Result<()> {
    if !is_legacy_kraddie_file(path) {
        return Ok(());
    }
    let target = path.with_extension(KRADDIE_FILE_EXTENSION);
    if target.exists() {
        return Ok(());
    }
    std::fs::rename(path, target)
}

fn settle_compass(profile: &Path) -> std::io::Result<()> {
    let legacy = profile.join(LEGACY_COMPASS_FILE);
    let target = profile.join(COMPASS_FILE);
    if !legacy.is_file() || target.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&legacy)?;
    if text.trim() == LEGACY_DEFAULT_COMPASS {
        return std::fs::remove_file(legacy);
    }
    std::fs::rename(legacy, target)
}

pub(super) fn is_legacy_kraddie_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == LEGACY_KRADDIE_FILE_EXTENSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const LEGACY_FILE: &str = "owl.kradpet";
    const OWNED_FILE: &str = "owl.automations.json";

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "krad-store-kraddie-folder-{name}-{}",
            std::process::id()
        ))
    }

    fn old_config(name: &str) -> Result<PathBuf, std::io::Error> {
        let config = scratch(name);
        let legacy = config.join(LEGACY_KRADDIES_DIR);
        std::fs::create_dir_all(legacy.join("owl"))?;
        std::fs::write(legacy.join(LEGACY_FILE), b"{}")?;
        std::fs::write(legacy.join(OWNED_FILE), b"{}")?;
        std::fs::write(
            legacy.join("owl").join(LEGACY_COMPASS_FILE),
            b"Run the tests.",
        )?;
        Ok(config)
    }

    #[test]
    fn the_old_folder_moves_once_and_its_files_take_their_new_names() -> TestResult {
        let config = old_config("move")?;
        let folder = config.join(KRADDIES_DIR);
        assert!(settle(&folder).is_empty());
        assert_eq!(leftover(&folder), None);
        assert!(folder.join("owl.kraddie").exists());
        assert!(folder.join(OWNED_FILE).exists());
        assert_eq!(
            std::fs::read_to_string(folder.join("owl").join(COMPASS_FILE))?,
            "Run the tests."
        );
        assert!(settle(&folder).is_empty());
        assert!(folder.join("owl.kraddie").exists());
        std::fs::remove_dir_all(config)?;
        Ok(())
    }

    #[test]
    fn both_folders_merge_without_overwriting_the_new_one() -> TestResult {
        let config = old_config("merge")?;
        let folder = config.join(KRADDIES_DIR);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(OWNED_FILE), b"new")?;
        assert!(settle(&folder).is_empty());
        assert_eq!(leftover(&folder), Some(config.join(LEGACY_KRADDIES_DIR)));
        assert_eq!(std::fs::read(folder.join(OWNED_FILE))?, b"new");
        assert!(folder.join("owl.kraddie").exists());
        assert!(config.join(LEGACY_KRADDIES_DIR).join(OWNED_FILE).exists());
        std::fs::remove_dir_all(config)?;
        Ok(())
    }

    #[test]
    fn the_untouched_old_template_is_dropped_instead_of_becoming_instructions() -> TestResult {
        let config = old_config("template")?;
        let legacy = config
            .join(LEGACY_KRADDIES_DIR)
            .join("owl")
            .join(LEGACY_COMPASS_FILE);
        std::fs::write(&legacy, format!("{LEGACY_DEFAULT_COMPASS}\n"))?;
        let folder = config.join(KRADDIES_DIR);
        assert!(settle(&folder).is_empty());
        assert!(!folder.join("owl").join(COMPASS_FILE).exists());
        assert!(!folder.join("owl").join(LEGACY_COMPASS_FILE).exists());
        std::fs::remove_dir_all(config)?;
        Ok(())
    }

    #[test]
    fn a_folder_of_another_name_has_nothing_to_migrate() {
        let config = scratch("empty");
        assert!(settle(&config.join(KRADDIES_DIR)).is_empty());
        assert_eq!(leftover(&config.join("shelf")), None);
        assert!(!config.exists());
    }
}
