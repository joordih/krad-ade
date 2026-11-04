mod embedded;

use gpui_kit::{img, px, AnyElement, Image, ImageFormat, IntoElement, Styled};
use krad_domain::{FileIconTheme, FolderKind, IconPaletteId, IconPalettes};
use std::collections::HashMap;
use std::sync::Arc;

const THEME: &str = include_str!("../../../assets/file-icons/theme.json");
const PALETTES: &str = include_str!("../../../assets/file-icons/palettes.json");

pub(crate) struct FileIcons {
    theme: FileIconTheme,
    images: HashMap<(IconPaletteId, &'static str), Arc<Image>>,
}

impl FileIcons {
    pub(crate) fn load() -> Option<Self> {
        let parsed = THEME
            .parse::<FileIconTheme>()
            .and_then(|theme| Ok((theme, PALETTES.parse::<IconPalettes>()?)));
        let (theme, palettes) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::error!(%error, "the embedded file icon theme does not read");
                return None;
            }
        };
        let images = IconPaletteId::ALL
            .into_iter()
            .flat_map(|palette| {
                let palettes = &palettes;
                embedded::ICONS.iter().map(move |(name, svg)| {
                    let colored = palettes.recolor(svg, palette).into_bytes();
                    (
                        (palette, *name),
                        Arc::new(Image::from_bytes(ImageFormat::Svg, colored)),
                    )
                })
            })
            .collect();
        Some(Self { theme, images })
    }

    fn element(&self, name: &str, palette: IconPaletteId, size: f32) -> Option<AnyElement> {
        let image = self.images.get(&(palette, name))?.clone();
        Some(img(image).size(px(size)).flex_none().into_any_element())
    }

    pub(crate) fn file(&self, name: &str, palette: IconPaletteId, size: f32) -> Option<AnyElement> {
        self.element(self.theme.file_icon(name), palette, size)
    }

    pub(crate) fn folder(
        &self,
        name: &str,
        (kind, expanded): (FolderKind, bool),
        palette: IconPaletteId,
        size: f32,
    ) -> Option<AnyElement> {
        self.element(self.theme.folder_icon(name, kind, expanded), palette, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_icon_the_theme_names_is_embedded_and_every_embedded_icon_is_a_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let theme: FileIconTheme = THEME.parse()?;
        let embedded: HashSet<&str> = embedded::ICONS.iter().map(|(name, _)| *name).collect();
        let missing: Vec<&str> = theme
            .icon_names()
            .filter(|name| !embedded.contains(name))
            .collect();
        assert_eq!(missing, Vec::<&str>::new());
        let folder =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/file-icons/icons");
        let on_disk = std::fs::read_dir(folder).map_or(0, Iterator::count);
        assert_eq!(on_disk, embedded::ICONS.len());
        assert!(FileIcons::load().is_some());
        Ok(())
    }
}
