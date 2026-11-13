use krad_domain::{HostPlatform, HostTimeouts, InstallId};
use krad_host::{new_install_id, HostContext, HOST_CLI_PROGRAM};
use std::path::PathBuf;

use super::bundled_cli;

#[derive(Clone, Debug, Default)]
pub struct HostTools {
    pub config_dir: Option<PathBuf>,
    pub install_id: Option<InstallId>,
    pub local_cli: Option<PathBuf>,
    pub host_clis: Vec<(HostPlatform, PathBuf)>,
}

impl HostTools {
    #[must_use]
    pub fn discover() -> Self {
        Self {
            config_dir: krad_store::config_dir(),
            install_id: krad_store::install_id(new_install_id),
            local_cli: local_cli(),
            host_clis: HostPlatform::ALL
                .into_iter()
                .filter_map(|platform| Some((platform, bundled_cli(platform)?)))
                .collect(),
        }
    }

    pub(crate) fn context(&self, timeouts: HostTimeouts) -> HostContext {
        HostContext {
            config_dir: self.config_dir.clone(),
            install_id: self.install_id.clone(),
            timeouts,
            transport: None,
            local_cli: self.local_cli.clone(),
            host_clis: self.host_clis.clone(),
            prompts: None,
        }
    }
}

fn local_cli() -> Option<PathBuf> {
    let file = format!("{HOST_CLI_PROGRAM}{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join(file))
        .filter(|path| path.is_file())
}
