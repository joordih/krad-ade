use krad_domain::InstallId;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::Path;

use crate::config_dir::config_dir;
use crate::layout::write_atomically;

const INSTALL_ID_FILE: &str = "install-id";

#[must_use]
pub fn install_id() -> Option<InstallId> {
    let dir = config_dir()?;
    read_or_create(&dir)
}

fn read_or_create(dir: &Path) -> Option<InstallId> {
    let path = dir.join(INSTALL_ID_FILE);
    if let Some(id) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| text.parse().ok())
    {
        return Some(id);
    }
    let id: InstallId = fresh_id().parse().ok()?;
    std::fs::create_dir_all(dir).ok()?;
    write_atomically(&path, id.as_str().as_bytes()).ok()?;
    std::fs::read_to_string(&path).ok()?.parse().ok()
}

fn random_word(salt: u8) -> u64 {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u8(salt);
    hasher.write_u32(std::process::id());
    hasher.finish()
}

fn fresh_id() -> String {
    format!("{:016x}{:016x}", random_word(0), random_word(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_is_created_once_and_then_kept() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("krad-install-id-{}", std::process::id()));
        let first = read_or_create(&dir).ok_or("no id")?;
        let second = read_or_create(&dir).ok_or("no id")?;
        assert_eq!(first, second);
        assert_eq!(first.as_str().len(), 32);
        assert_ne!(fresh_id(), fresh_id());
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
}
