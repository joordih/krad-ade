use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;

use crate::components::IconName;

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match IconName::ALL.iter().find(|name| name.path() == path) {
            Some(name) => Ok(Some(Cow::Borrowed(name.bytes()))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut entries: Vec<SharedString> = IconName::ALL
            .iter()
            .map(|name| name.path())
            .filter(|asset| asset.starts_with(path))
            .map(SharedString::from)
            .collect();
        entries.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_loads_through_the_asset_source() -> Result<()> {
        for name in IconName::ALL {
            let loaded = AppAssets.load(name.path())?;
            assert_eq!(loaded.as_deref(), Some(name.bytes()), "{name:?}");
        }
        Ok(())
    }
}
