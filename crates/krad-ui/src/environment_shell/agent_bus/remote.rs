use krad_agents::{
    endpoint_file_env, member_token, token_file_env, Caller, Endpoint, ASKPASS_VERB, CLI_PROGRAM,
};
use krad_domain::{claude_hook_settings, CloudRunnerId};
use krad_host::mux::ASKPASS_PROGRAM;
use krad_host::{FileMode, HostError, HostLink, RemoteForward, RemoteLink, LOOPBACK};
use std::path::PathBuf;
use std::sync::Arc;

use super::super::host_cli::{upload_host_cli, HostCliError};

const TOKEN_SUFFIX: &str = ".token";
const ENDPOINT_EXTENSION: &str = "endpoint";
const RUNNER_SLUG_PREFIX: &str = "runner-";
const PATH_VAR: &str = "PATH";
const SLUG_ESCAPE: char = '_';
const SCRIPT_SHEBANG: &str = "#!/bin/sh";

pub(in crate::environment_shell) struct RemoteBus {
    forward: RemoteForward,
    bin_dir: PathBuf,
    token_file: PathBuf,
    endpoint_file: PathBuf,
    pub(in crate::environment_shell) claude_settings: PathBuf,
}

#[derive(Default)]
pub(in crate::environment_shell) enum RemoteBusSlot {
    #[default]
    Absent,
    Preparing,
    Ready(RemoteBus),
}

impl RemoteBusSlot {
    pub(in crate::environment_shell) const fn ready(&self) -> Option<&RemoteBus> {
        match self {
            Self::Ready(bus) => Some(bus),
            Self::Absent | Self::Preparing => None,
        }
    }

    pub(in crate::environment_shell) const fn is_preparing(&self) -> bool {
        matches!(self, Self::Preparing)
    }
}

pub(in crate::environment_shell) struct RemoteBusPlan {
    pub(in crate::environment_shell) ssh: Arc<RemoteLink>,
    pub(in crate::environment_shell) link: HostLink,
    pub(in crate::environment_shell) slug: String,
    pub(in crate::environment_shell) local_port: u16,
    pub(in crate::environment_shell) token: String,
    pub(in crate::environment_shell) preferred_port: Option<u16>,
}

impl RemoteBusPlan {
    pub(in crate::environment_shell) fn new(
        ssh: Arc<RemoteLink>,
        link: HostLink,
        environment: &str,
        endpoint: &Endpoint,
        preferred_port: Option<u16>,
    ) -> Self {
        Self {
            ssh,
            link,
            slug: environment_slug(environment),
            local_port: endpoint.address.port(),
            token: member_token(endpoint).to_owned(),
            preferred_port,
        }
    }

    pub(in crate::environment_shell) fn for_runner(
        ssh: Arc<RemoteLink>,
        link: HostLink,
        runner: CloudRunnerId,
        endpoint: &Endpoint,
        preferred_port: Option<u16>,
    ) -> Self {
        Self {
            ssh,
            link,
            slug: runner_slug(runner),
            local_port: endpoint.address.port(),
            token: member_token(endpoint).to_owned(),
            preferred_port,
        }
    }

    pub(in crate::environment_shell) fn prepare(self) -> Result<RemoteBus, HostCliError> {
        let platform = self
            .ssh
            .platform()
            .ok_or(HostCliError::Host(HostError::NotConnected))?;
        let layout = self
            .ssh
            .layout()
            .ok_or(HostCliError::Host(HostError::NotConnected))?;
        let bin_dir = layout.bin_dir();
        upload_host_cli(&self.link, platform, &bin_dir)?;
        self.link.write_with_mode(
            &bin_dir.join(ASKPASS_PROGRAM),
            askpass_script().as_bytes(),
            FileMode::EXECUTABLE,
        )?;
        let bus_dir = layout.bus_dir()?;
        self.link
            .create_dir_with_mode(&bus_dir, FileMode::PRIVATE_DIR)?;
        let token_file = bus_dir.join(format!("{}{TOKEN_SUFFIX}", self.slug));
        self.link
            .write_with_mode(&token_file, self.token.as_bytes(), FileMode::PRIVATE_FILE)?;
        let claude_settings = layout.claude_hooks_file()?;
        self.link.write_with_mode(
            &claude_settings,
            claude_hook_settings(CLI_PROGRAM).as_bytes(),
            FileMode::PRIVATE_FILE,
        )?;
        let forward = self
            .ssh
            .forward_remote(self.local_port, self.preferred_port)?;
        let endpoint_file = token_file.with_extension(ENDPOINT_EXTENSION);
        self.link.write_with_mode(
            &endpoint_file,
            loopback_address(forward.remote_port).as_bytes(),
            FileMode::PRIVATE_FILE,
        )?;
        Ok(RemoteBus {
            forward,
            bin_dir,
            token_file,
            endpoint_file,
            claude_settings,
        })
    }
}

impl RemoteBus {
    pub(in crate::environment_shell) fn caller_vars(
        &self,
        caller: &Caller,
    ) -> Vec<(String, String)> {
        let address = loopback_address(self.forward.remote_port);
        let mut vars = token_file_env(&address, &self.token_file, caller);
        vars.push(self.path_var());
        vars
    }

    pub(in crate::environment_shell) fn detached_caller_vars(
        &self,
        caller: &Caller,
    ) -> Vec<(String, String)> {
        let mut vars = endpoint_file_env(&self.endpoint_file, &self.token_file, caller);
        vars.push(self.path_var());
        vars
    }

    fn path_var(&self) -> (String, String) {
        (
            PATH_VAR.to_owned(),
            self.bin_dir.to_string_lossy().into_owned(),
        )
    }

    pub(in crate::environment_shell) fn askpass(
        &self,
        caller: &Caller,
    ) -> (PathBuf, Vec<(String, String)>) {
        let address = loopback_address(self.forward.remote_port);
        (
            self.bin_dir.join(ASKPASS_PROGRAM),
            token_file_env(&address, &self.token_file, caller),
        )
    }

    pub(in crate::environment_shell) const fn remote_port(&self) -> u16 {
        self.forward.remote_port
    }

    pub(in crate::environment_shell) fn is_alive(&mut self) -> bool {
        self.forward.is_alive()
    }
}

fn loopback_address(port: u16) -> String {
    format!("{LOOPBACK}:{port}")
}

fn runner_slug(runner: CloudRunnerId) -> String {
    format!("{RUNNER_SLUG_PREFIX}{runner}")
}

fn askpass_script() -> String {
    format!("{SCRIPT_SHEBANG}\nexec \"${{0%/*}}/{CLI_PROGRAM}\" {ASKPASS_VERB} \"$@\"\n")
}

fn environment_slug(environment: &str) -> String {
    environment
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                char::from(byte).to_string()
            } else {
                format!("{SLUG_ESCAPE}{byte:02x}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_askpass_script_hands_the_prompt_to_the_cli_beside_it() {
        assert_eq!(
            askpass_script(),
            "#!/bin/sh\nexec \"${0%/*}/krad-ade-agents\" askpass \"$@\"\n"
        );
    }

    #[test]
    fn only_a_preparing_bus_holds_agents_back() {
        assert!(RemoteBusSlot::Preparing.is_preparing());
        assert!(!RemoteBusSlot::Absent.is_preparing());
        assert!(RemoteBusSlot::Preparing.ready().is_none());
    }

    #[test]
    fn environment_names_become_safe_file_names() {
        assert_eq!(environment_slug("api work/2"), "api_20work_2f2");
        assert_eq!(environment_slug("../etc"), "_2e_2e_2fetc");
        assert_ne!(environment_slug("a b"), environment_slug("a-b"));
        assert_ne!(environment_slug("a_20b"), environment_slug("a b"));
    }

    #[test]
    fn a_runner_slug_never_matches_an_environment_slug() {
        let runner = runner_slug(CloudRunnerId(7));
        assert_eq!(runner, "runner-7");
        assert_ne!(environment_slug("runner-7"), runner);
        assert_eq!(loopback_address(41_000), "127.0.0.1:41000");
    }
}
