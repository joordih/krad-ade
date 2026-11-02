use krad_domain::InstallId;
use krad_host::HostPlatform;
use std::path::PathBuf;

use super::super::agent_bus::bundled_cli;
use super::local_cli;

#[derive(Clone, Debug, Default)]
pub struct HostTools {
    pub(in crate::environment_shell) install_id: Option<InstallId>,
    pub(in crate::environment_shell) local_cli: Option<PathBuf>,
    pub(in crate::environment_shell) host_clis: Vec<(HostPlatform, PathBuf)>,
}

impl HostTools {
    #[must_use]
    pub fn discover() -> Self {
        Self {
            install_id: krad_store::install_id(krad_host::new_install_id),
            local_cli: local_cli(),
            host_clis: HostPlatform::ALL
                .into_iter()
                .filter_map(|platform| Some((platform, bundled_cli(platform)?)))
                .collect(),
        }
    }
}
