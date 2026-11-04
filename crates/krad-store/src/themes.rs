use crate::config_dir::config_dir;
use crate::layout::write_atomically;
use krad_domain::Theme;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

const THEMES_DIR: &str = "themes";
const THEME_EXTENSION: &str = "json";
const NO_CONFIG_DIR: &str = "no config directory";

fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(THEMES_DIR))
}

fn themes_in(dir: &Path) -> Vec<Theme> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut themes: Vec<Theme> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == THEME_EXTENSION)
        })
        .filter_map(|path| std::fs::read(path).ok())
        .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
        .collect();
    themes.sort_by_key(|theme| theme.name.to_lowercase());
    themes
}

#[must_use]
pub fn read_user_themes() -> Vec<Theme> {
    themes_dir().map_or_else(Vec::new, |dir| themes_in(&dir))
}

/// # Errors
///
/// Fails when there is no config folder, or the theme cannot be written.
pub fn write_user_theme(theme: &Theme) -> std::io::Result<()> {
    let dir = themes_dir().ok_or_else(|| Error::new(ErrorKind::NotFound, NO_CONFIG_DIR))?;
    let path = dir.join(&theme.id.0).with_extension(THEME_EXTENSION);
    let bytes = serde_json::to_vec_pretty(theme)
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;
    write_atomically(&path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{ThemeAppearance, ThemeId};

    #[test]
    fn only_readable_json_themes_are_listed_by_name() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("krad-store-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let library = include_str!("../../../assets/themes/library/dracula.json");
        let mut theme: Theme = serde_json::from_str(library)?;
        theme.id = ThemeId("mine".to_owned());
        theme.name = "Mine".to_owned();
        std::fs::write(dir.join("mine.json"), serde_json::to_vec(&theme)?)?;
        std::fs::write(dir.join("broken.json"), b"{")?;
        std::fs::write(dir.join("notes.txt"), b"hi")?;
        let themes = themes_in(&dir);
        std::fs::remove_dir_all(&dir)?;
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].appearance, ThemeAppearance::Dark);
        Ok(())
    }
}
