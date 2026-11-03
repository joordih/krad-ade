use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::layout::APP_VERSION;

pub const ASKPASS_PROGRAM: &str = "krad-ade-askpass";
const ALIAS_DIR: &str = "bin";

#[must_use]
pub fn is_askpass_invocation(program: &OsStr) -> bool {
    Path::new(program)
        .file_stem()
        .is_some_and(|stem| stem == ASKPASS_PROGRAM)
}

pub(crate) fn askpass_alias(local_cli: &Path, config_dir: Option<&Path>) -> io::Result<PathBuf> {
    let dir = config_dir
        .map(|dir| dir.join(ALIAS_DIR).join(APP_VERSION))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config folder for askpass"))?;
    let alias = dir.join(format!("{ASKPASS_PROGRAM}{}", std::env::consts::EXE_SUFFIX));
    if alias.is_file() {
        return Ok(alias);
    }
    fs::create_dir_all(&dir)?;
    let staged = dir.join(format!(
        "{ASKPASS_PROGRAM}.{}{}",
        std::process::id(),
        std::env::consts::EXE_SUFFIX
    ));
    fs::hard_link(local_cli, &staged).or_else(|_| fs::copy(local_cli, &staged).map(drop))?;
    fs::rename(&staged, &alias).inspect_err(|_| {
        fs::remove_file(&staged).ok();
    })?;
    Ok(alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_askpass_alias_answers_prompts() {
        assert!(is_askpass_invocation(OsStr::new(
            "/cfg/bin/0.2.0/krad-ade-askpass"
        )));
        assert!(is_askpass_invocation(OsStr::new(r"krad-ade-askpass.exe")));
        assert!(!is_askpass_invocation(OsStr::new(
            "/usr/local/bin/krad-ade-agents"
        )));
    }

    #[test]
    fn the_alias_is_made_once_next_to_the_config() -> io::Result<()> {
        let base = std::env::temp_dir().join(format!("krad-askpass-{}", std::process::id()));
        fs::create_dir_all(&base)?;
        let cli = base.join("krad-ade-agents");
        fs::write(&cli, b"cli")?;
        let alias = askpass_alias(&cli, Some(&base))?;
        assert!(is_askpass_invocation(alias.as_os_str()));
        assert_eq!(fs::read(&alias)?, b"cli");
        assert_eq!(askpass_alias(&cli, Some(&base))?, alias);
        fs::remove_dir_all(base)
    }
}
