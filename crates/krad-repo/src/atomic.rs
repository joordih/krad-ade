use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::RepoError;

const TEMP_SUFFIX: &str = ".tmp";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub(crate) fn write_atomically(path: &Path, text: &str) -> Result<(), RepoError> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(format!(
        ".{}-{}{TEMP_SUFFIX}",
        process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let temp = PathBuf::from(temp);
    fs::write(&temp, text)?;
    fs::rename(&temp, path)?;
    Ok(())
}
