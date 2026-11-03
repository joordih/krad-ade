use krad_domain::{HostTimeouts, InstallId};
use std::path::PathBuf;

use crate::mux::PromptSender;
use crate::platform::HostPlatform;
use crate::ssh::Transport;

#[derive(Clone, Debug, Default)]
pub struct HostContext {
    pub config_dir: Option<PathBuf>,
    pub install_id: Option<InstallId>,
    pub timeouts: HostTimeouts,
    pub transport: Option<Transport>,
    pub local_cli: Option<PathBuf>,
    pub host_clis: Vec<(HostPlatform, PathBuf)>,
    pub prompts: Option<PromptSender>,
}
