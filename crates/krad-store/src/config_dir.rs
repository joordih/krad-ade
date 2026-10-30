use std::ffi::OsString;
use std::path::PathBuf;

const XDG_CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
const HOME_VAR: &str = "HOME";
const APPDATA_VAR: &str = "APPDATA";
const CONFIG_FALLBACK_DIR: &str = ".config";
const GATEWAY_SESSIONS_DIR: &str = "gateway-sessions";

pub const APP_NAME: &str = "krad-ade";

#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = non_empty_var(XDG_CONFIG_HOME_VAR) {
        return Some(PathBuf::from(dir).join(APP_NAME));
    }
    if let Some(home) = non_empty_var(HOME_VAR) {
        return Some(PathBuf::from(home).join(CONFIG_FALLBACK_DIR).join(APP_NAME));
    }
    non_empty_var(APPDATA_VAR).map(|appdata| PathBuf::from(appdata).join(APP_NAME))
}

#[must_use]
pub fn gateway_sessions_dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(GATEWAY_SESSIONS_DIR))
}

pub(crate) fn non_empty_var(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}
