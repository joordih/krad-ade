#[cfg(feature = "desktop")]
pub fn api_file() -> Option<krad_agents::ApiFile> {
    krad_store::config_dir().map(|dir| krad_agents::ApiFile::in_config(&dir))
}

#[cfg(not(feature = "desktop"))]
pub const fn api_file() -> Option<krad_agents::ApiFile> {
    None
}
