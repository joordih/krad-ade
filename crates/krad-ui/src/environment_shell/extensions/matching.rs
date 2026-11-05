use krad_extensions::api::manifest::{Activation, ExtensionManifest, MenuContribution, MenuTarget};
use std::path::Path;

const WILDCARD_PREFIX: &str = "*.";

pub(super) fn file_matches(patterns: &[String], path: &Path) -> bool {
    if patterns.is_empty() {
        return true;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    patterns
        .iter()
        .any(|pattern| match pattern.strip_prefix(WILDCARD_PREFIX) {
            Some(extension) => path
                .extension()
                .and_then(|found| found.to_str())
                .is_some_and(|found| found == extension),
            None => pattern == name,
        })
}

pub(super) fn menu_applies(
    menu: &MenuContribution,
    target: MenuTarget,
    path: Option<&Path>,
) -> bool {
    if menu.target != target {
        return false;
    }
    match path {
        Some(path) if matches!(target, MenuTarget::File) => file_matches(&menu.files, path),
        _ => true,
    }
}

pub(super) fn activates_on_startup(manifest: &ExtensionManifest) -> bool {
    manifest.activation.contains(&Activation::Startup)
}

pub(super) fn activates_on_environment(manifest: &ExtensionManifest) -> bool {
    manifest.activation.contains(&Activation::EnvironmentOpened)
}

pub(super) fn workspace_markers(manifest: &ExtensionManifest) -> Vec<String> {
    manifest
        .activation
        .iter()
        .filter_map(|activation| match activation {
            Activation::WorkspaceContains(file) => Some(file.clone()),
            Activation::Startup | Activation::EnvironmentOpened => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extensions::api::CommandId;

    fn menu(
        target: MenuTarget,
        files: &[&str],
    ) -> Result<MenuContribution, krad_extensions::api::IdError> {
        Ok(MenuContribution {
            command: "refresh".parse::<CommandId>()?,
            target,
            files: files.iter().map(|file| (*file).to_owned()).collect(),
        })
    }

    #[test]
    fn file_patterns_match_names_and_extensions() {
        let patterns = ["Dockerfile".to_owned(), "*.yml".to_owned()];
        assert!(file_matches(&patterns, Path::new("/repo/Dockerfile")));
        assert!(file_matches(&patterns, Path::new("/repo/compose.yml")));
        assert!(!file_matches(&patterns, Path::new("/repo/compose.yaml")));
        assert!(file_matches(&[], Path::new("/repo/anything")));
    }

    #[test]
    fn a_menu_shows_only_on_its_target_and_files() -> Result<(), krad_extensions::api::IdError> {
        let docker = menu(MenuTarget::File, &["Dockerfile"])?;
        assert!(menu_applies(
            &docker,
            MenuTarget::File,
            Some(Path::new("/x/Dockerfile"))
        ));
        assert!(!menu_applies(
            &docker,
            MenuTarget::File,
            Some(Path::new("/x/main.rs"))
        ));
        assert!(!menu_applies(
            &docker,
            MenuTarget::Folder,
            Some(Path::new("/x"))
        ));
        let tabs = menu(MenuTarget::Tab, &[])?;
        assert!(menu_applies(&tabs, MenuTarget::Tab, None));
        Ok(())
    }
}
