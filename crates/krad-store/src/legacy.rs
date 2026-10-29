use crate::config_dir::{config_dir, config_dir_named};
use crate::gitlab_login::GITLAB_VAULT;
use crate::settings::read_settings;
use crate::vault::adopt_keyring_login;
use std::io::ErrorKind;
use std::path::Path;

const LEGACY_APP_NAME: &str = "rmd-ade";
const LEGACY_KEYRING_SERVICE: &str = "rmd-ade-gitlab";
const KEYRING_THREAD_NAME: &str = "legacy-gitlab-login";

pub fn migrate_legacy_data() {
    if let (Some(legacy), Some(current)) = (config_dir_named(LEGACY_APP_NAME), config_dir()) {
        drop(adopt_legacy_dir(&legacy, &current));
    }
    let host = read_settings().gitlab.host;
    if host.is_empty() {
        return;
    }
    drop(
        std::thread::Builder::new()
            .name(KEYRING_THREAD_NAME.to_owned())
            .spawn(move || {
                drop(adopt_keyring_login(
                    &GITLAB_VAULT,
                    LEGACY_KEYRING_SERVICE,
                    &host,
                ));
            }),
    );
}

pub(crate) fn adopt_legacy_dir(legacy: &Path, current: &Path) -> std::io::Result<()> {
    if exists(current)? || !exists(legacy)? {
        return Ok(());
    }
    std::fs::rename(legacy, current)
}

fn exists(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> std::io::Result<PathBuf> {
        let dir =
            std::env::temp_dir().join(format!("krad-store-legacy-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).unwrap_or_default();
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn a_legacy_folder_moves_when_there_is_no_current_one() -> std::io::Result<()> {
        let root = temp_root("moves")?;
        let (legacy, current) = (root.join(LEGACY_APP_NAME), root.join(crate::APP_NAME));
        std::fs::create_dir_all(&legacy)?;
        std::fs::write(legacy.join("settings.json"), "{}")?;
        adopt_legacy_dir(&legacy, &current)?;
        assert!(!legacy.exists());
        assert_eq!(
            std::fs::read_to_string(current.join("settings.json"))?,
            "{}"
        );
        std::fs::remove_dir_all(root)
    }

    #[test]
    fn a_current_folder_is_never_replaced() -> std::io::Result<()> {
        let root = temp_root("keeps")?;
        let (legacy, current) = (root.join(LEGACY_APP_NAME), root.join(crate::APP_NAME));
        std::fs::create_dir_all(&legacy)?;
        std::fs::create_dir_all(&current)?;
        std::fs::write(legacy.join("settings.json"), "old")?;
        std::fs::write(current.join("settings.json"), "new")?;
        adopt_legacy_dir(&legacy, &current)?;
        assert_eq!(
            std::fs::read_to_string(legacy.join("settings.json"))?,
            "old"
        );
        assert_eq!(
            std::fs::read_to_string(current.join("settings.json"))?,
            "new"
        );
        std::fs::remove_dir_all(root)
    }

    #[test]
    fn nothing_happens_without_a_legacy_folder() -> std::io::Result<()> {
        let root = temp_root("absent")?;
        let current = root.join(crate::APP_NAME);
        adopt_legacy_dir(&root.join(LEGACY_APP_NAME), &current)?;
        assert!(!current.exists());
        std::fs::remove_dir_all(root)
    }
}
