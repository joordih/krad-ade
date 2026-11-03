use krad_domain::InstallId;
use std::path::Path;

use crate::config_dir::config_dir;
use crate::layout::write_atomically;

const INSTALL_ID_FILE: &str = "install-id";

#[must_use]
pub fn install_id(fresh: impl FnOnce() -> Option<InstallId>) -> Option<InstallId> {
    let dir = config_dir()?;
    read_or_create(&dir, fresh)
}

fn read_or_create(dir: &Path, fresh: impl FnOnce() -> Option<InstallId>) -> Option<InstallId> {
    let path = dir.join(INSTALL_ID_FILE);
    if let Some(id) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| text.parse().ok())
    {
        return Some(id);
    }
    let id = fresh()?;
    std::fs::create_dir_all(dir).ok()?;
    write_atomically(&path, id.as_str().as_bytes()).ok()?;
    std::fs::read_to_string(&path).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_is_created_once_and_then_kept() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("krad-install-id-{}", std::process::id()));
        let given = || "0123456789abcdef0123456789abcdef".parse().ok();
        let first = read_or_create(&dir, given).ok_or("no id")?;
        let second = read_or_create(&dir, || None).ok_or("no id")?;
        assert_eq!(first, second);
        assert_eq!(first.as_str(), "0123456789abcdef0123456789abcdef");
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
}
