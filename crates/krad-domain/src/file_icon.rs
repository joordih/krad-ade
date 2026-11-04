use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

const DEFAULT_FILE_ICON: &str = "_file";
const SUFFIX_SEPARATOR: char = '.';

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum FileIconColors {
    #[default]
    Automatic,
    Base,
    Light,
    Soft,
    Warm,
}

impl FileIconColors {
    pub const ALL: [Self; 5] = [
        Self::Automatic,
        Self::Base,
        Self::Light,
        Self::Soft,
        Self::Warm,
    ];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Base => "Base",
            Self::Light => "Light",
            Self::Soft => "Soft",
            Self::Warm => "Warm",
        }
    }

    #[must_use]
    pub const fn palette(self, dark: bool) -> IconPaletteId {
        match self {
            Self::Automatic if dark => IconPaletteId::Base,
            Self::Automatic | Self::Light => IconPaletteId::Light,
            Self::Base => IconPaletteId::Base,
            Self::Soft => IconPaletteId::Soft,
            Self::Warm => IconPaletteId::Warm,
        }
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconPaletteId {
    Base,
    Light,
    Soft,
    Warm,
}

impl IconPaletteId {
    pub const ALL: [Self; 4] = [Self::Base, Self::Light, Self::Soft, Self::Warm];
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct FolderIcons {
    pub collapsed: String,
    pub expanded: String,
}

impl FolderIcons {
    #[must_use]
    pub fn for_state(&self, expanded: bool) -> &str {
        if expanded {
            &self.expanded
        } else {
            &self.collapsed
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct FileIconTheme {
    directory_icons: FolderIcons,
    package_icons: FolderIcons,
    file_stems: HashMap<String, String>,
    file_suffixes: HashMap<String, String>,
    named_directory_icons: HashMap<String, FolderIcons>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderKind {
    Plain,
    Package,
}

impl FileIconTheme {
    #[must_use]
    pub fn file_icon(&self, name: &str) -> &str {
        let name = name.to_lowercase();
        if let Some(icon) = self.file_stems.get(&name) {
            return icon;
        }
        name.match_indices(SUFFIX_SEPARATOR)
            .find_map(|(at, _)| self.file_suffixes.get(&name[at + 1..]))
            .map_or(DEFAULT_FILE_ICON, String::as_str)
    }

    #[must_use]
    pub fn folder_icon(&self, name: &str, kind: FolderKind, expanded: bool) -> &str {
        let icons = match kind {
            FolderKind::Package => &self.package_icons,
            FolderKind::Plain => self
                .named_directory_icons
                .get(&name.to_lowercase())
                .unwrap_or(&self.directory_icons),
        };
        icons.for_state(expanded)
    }

    pub fn icon_names(&self) -> impl Iterator<Item = &str> {
        let folders = self
            .named_directory_icons
            .values()
            .chain([&self.directory_icons, &self.package_icons])
            .flat_map(|icons| [icons.collapsed.as_str(), icons.expanded.as_str()]);
        self.file_stems
            .values()
            .chain(self.file_suffixes.values())
            .map(String::as_str)
            .chain(folders)
            .chain([DEFAULT_FILE_ICON])
    }
}

impl FromStr for FileIconTheme {
    type Err = serde_json::Error;

    fn from_str(json: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(json)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct IconPalettes {
    base: HashMap<String, String>,
    light: HashMap<String, String>,
    soft: HashMap<String, String>,
    warm: HashMap<String, String>,
}

impl IconPalettes {
    fn colors(&self, palette: IconPaletteId) -> &HashMap<String, String> {
        match palette {
            IconPaletteId::Base => &self.base,
            IconPaletteId::Light => &self.light,
            IconPaletteId::Soft => &self.soft,
            IconPaletteId::Warm => &self.warm,
        }
    }

    #[must_use]
    pub fn recolor(&self, svg: &str, palette: IconPaletteId) -> String {
        let chosen = self.colors(palette);
        self.base
            .iter()
            .filter_map(|(key, base)| Some((base, chosen.get(key)?)))
            .filter(|(base, color)| base != color)
            .fold(svg.to_owned(), |svg, (base, color)| {
                svg.replace(base.as_str(), color)
            })
    }
}

impl FromStr for IconPalettes {
    type Err = serde_json::Error;

    fn from_str(json: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Result<FileIconTheme, serde_json::Error> {
        r#"{
            "directory_icons": {"collapsed": "_folder", "expanded": "_folder_open"},
            "package_icons": {"collapsed": "folder_package", "expanded": "folder_package_open"},
            "file_stems": {"pom.xml": "maven", "dockerfile": "docker"},
            "file_suffixes": {"java": "java", "test.ts": "test-blue", "ts": "typescript"},
            "named_directory_icons": {"src": {"collapsed": "folder_source", "expanded": "folder_source_open"}}
        }"#
        .parse()
    }

    #[test]
    fn a_file_takes_its_whole_name_then_its_longest_suffix() -> Result<(), serde_json::Error> {
        let theme = theme()?;
        assert_eq!(theme.file_icon("pom.xml"), "maven");
        assert_eq!(theme.file_icon("Dockerfile"), "docker");
        assert_eq!(theme.file_icon("auth.test.ts"), "test-blue");
        assert_eq!(theme.file_icon("Main.java"), "java");
        assert_eq!(theme.file_icon("notes.unknown"), "_file");
        Ok(())
    }

    #[test]
    fn folders_take_their_named_icon_and_packages_their_own() -> Result<(), serde_json::Error> {
        let theme = theme()?;
        assert_eq!(
            theme.folder_icon("src", FolderKind::Plain, true),
            "folder_source_open"
        );
        assert_eq!(
            theme.folder_icon("auth", FolderKind::Plain, false),
            "_folder"
        );
        assert_eq!(
            theme.folder_icon("src", FolderKind::Package, false),
            "folder_package"
        );
        Ok(())
    }

    #[test]
    fn a_palette_swaps_each_base_color_it_changes() -> Result<(), serde_json::Error> {
        let palettes: IconPalettes = r##"{
            "base": {"blue": "#5D80FF", "gray": "#9595AC"},
            "light": {"blue": "#4565D6"},
            "soft": {},
            "warm": {"blue": "#687CED", "gray": "#9A90A4"}
        }"##
        .parse()?;
        let svg = r##"<path fill="#5D80FF"/><path fill="#9595AC"/>"##;
        assert_eq!(
            palettes.recolor(svg, IconPaletteId::Light),
            r##"<path fill="#4565D6"/><path fill="#9595AC"/>"##
        );
        assert_eq!(palettes.recolor(svg, IconPaletteId::Soft), svg);
        assert_eq!(
            palettes.recolor(svg, IconPaletteId::Warm),
            r##"<path fill="#687CED"/><path fill="#9A90A4"/>"##
        );
        assert_eq!(FileIconColors::Automatic.palette(true), IconPaletteId::Base);
        assert_eq!(
            FileIconColors::Automatic.palette(false),
            IconPaletteId::Light
        );
        Ok(())
    }
}
