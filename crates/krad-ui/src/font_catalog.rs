#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

use gpui_kit::App;

#[cfg(target_os = "macos")]
use macos::system_families;
#[cfg(not(target_os = "macos"))]
use other::system_families;

use crate::theme::{BUNDLED_MONO_FAMILIES, UI_FONT_FAMILY};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FamilyKind {
    Monospace,
    Text,
    Decorative,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FontCatalog {
    pub(crate) monospace: Vec<String>,
    pub(crate) interface: Vec<String>,
}

const HIDDEN_PREFIX: char = '.';

fn catalog_of(families: impl IntoIterator<Item = (String, FamilyKind)>) -> FontCatalog {
    let mut catalog = FontCatalog::default();
    for (family, kind) in families {
        if family.starts_with(HIDDEN_PREFIX) || family == UI_FONT_FAMILY {
            continue;
        }
        match kind {
            FamilyKind::Monospace => catalog.monospace.push(family),
            FamilyKind::Text => catalog.interface.push(family),
            FamilyKind::Decorative => {}
        }
    }
    catalog.monospace.extend(
        BUNDLED_MONO_FAMILIES
            .iter()
            .map(|family| (*family).to_owned()),
    );
    for list in [&mut catalog.monospace, &mut catalog.interface] {
        list.sort_by_key(|family| family.to_lowercase());
        list.dedup();
    }
    catalog
}

pub(crate) fn font_catalog(cx: &App) -> FontCatalog {
    catalog_of(system_families(cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decorative_and_hidden_families_are_left_out_and_lists_are_sorted() {
        let catalog = catalog_of([
            ("Menlo".to_owned(), FamilyKind::Monospace),
            ("Zapfino".to_owned(), FamilyKind::Decorative),
            ("Helvetica".to_owned(), FamilyKind::Text),
            (".Hidden UI".to_owned(), FamilyKind::Text),
            ("avenir".to_owned(), FamilyKind::Text),
        ]);
        assert_eq!(catalog.interface, ["avenir", "Helvetica"]);
        assert!(catalog.monospace.contains(&"Menlo".to_owned()));
        assert!(!catalog.interface.contains(&"Zapfino".to_owned()));
    }
}
