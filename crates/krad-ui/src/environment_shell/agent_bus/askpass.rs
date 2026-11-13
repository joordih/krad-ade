use krad_agents::{terminal_env, Caller, ErrorCode, Response};
use krad_domain::{parse_wsl_path, EnvironmentName};
use krad_host::mux::askpass_alias;
use krad_repo::Askpass;
use krad_runtime::remote_askpass;
use krad_store::config_dir;
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;

const GIT_ONLY: &str = "Only git asks for a sign-in through the bus";

pub(super) fn git_only() -> Response {
    Response::error(ErrorCode::Forbidden, GIT_ONLY)
}

pub(in crate::environment_shell) enum AskpassPlan {
    Off,
    Local {
        cli: PathBuf,
        config: Option<PathBuf>,
        bus: Vec<(String, String)>,
    },
    Ready(Askpass),
}

impl AskpassPlan {
    pub(in crate::environment_shell) fn resolve(self) -> Askpass {
        match self {
            Self::Off => Askpass::default(),
            Self::Ready(askpass) => askpass,
            Self::Local { cli, config, bus } => askpass_alias(&cli, config.as_deref()).map_or_else(
                |error| {
                    tracing::debug!(%error, "git runs without an askpass program");
                    Askpass::default()
                },
                |program| Askpass::new(program, bus),
            ),
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn git_askpass(
        &self,
        env: &EnvironmentName,
        root: &Path,
    ) -> AskpassPlan {
        let (Some(bus), Some(live)) = (self.runtime.handle.bus(), self.envs.get(env)) else {
            return AskpassPlan::Off;
        };
        let caller = Caller::Git(env.as_str().to_owned());
        if !live.data.host.is_local() {
            return self.remote_bus(env).map_or(AskpassPlan::Off, |remote| {
                let (program, vars) = remote_askpass(remote, &caller);
                AskpassPlan::Ready(Askpass::new(program, vars))
            });
        }
        if parse_wsl_path(root).is_some() {
            return AskpassPlan::Off;
        }
        self.host_tools
            .local_cli
            .clone()
            .map_or(AskpassPlan::Off, |cli| AskpassPlan::Local {
                cli,
                config: config_dir(),
                bus: terminal_env(&bus.endpoint, &caller),
            })
    }
}
