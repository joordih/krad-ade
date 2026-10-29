use std::ffi::OsString;
use std::path::PathBuf;

const XDG_CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
const HOME_VAR: &str = "HOME";
const USERPROFILE_VAR: &str = "USERPROFILE";
const APPDATA_VAR: &str = "APPDATA";
const CONFIG_FALLBACK_DIR: &str = ".config";
const OPENCODE_DIR: &str = "opencode";

pub const APP_NAME: &str = "krad-ade";

#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    config_dir_named(APP_NAME)
}

fn config_dir_named(name: &str) -> Option<PathBuf> {
    if let Some(dir) = non_empty_var(XDG_CONFIG_HOME_VAR) {
        return Some(PathBuf::from(dir).join(name));
    }
    if let Some(home) = non_empty_var(HOME_VAR) {
        return Some(PathBuf::from(home).join(CONFIG_FALLBACK_DIR).join(name));
    }
    non_empty_var(APPDATA_VAR).map(|appdata| PathBuf::from(appdata).join(name))
}

#[must_use]
pub fn opencode_config_dir() -> Option<PathBuf> {
    non_empty_var(XDG_CONFIG_HOME_VAR)
        .map(PathBuf::from)
        .or_else(|| {
            non_empty_var(HOME_VAR)
                .or_else(|| non_empty_var(USERPROFILE_VAR))
                .map(|home| PathBuf::from(home).join(CONFIG_FALLBACK_DIR))
        })
        .map(|config| config.join(OPENCODE_DIR))
}

pub(crate) fn non_empty_var(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}
