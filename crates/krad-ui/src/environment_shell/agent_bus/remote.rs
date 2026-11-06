use krad_agents::{member_token, token_file_env, Caller, Endpoint, ASKPASS_VERB, CLI_PROGRAM};
use krad_domain::claude_hook_settings;
use krad_host::mux::ASKPASS_PROGRAM;
use krad_host::{
    FileMode, HostCommand, HostError, HostLink, HostPlatform, RemoteForward, RemoteLink,
    HOST_CLI_BUILD, LOOPBACK,
};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const VERSION_FLAG: &str = "--version";
const TOKEN_SUFFIX: &str = ".token";
const BUNDLE_AGENTS_DIR: &str = "agents";
const MACOS_RESOURCES: [&str; 2] = ["..", "Resources"];
const BUILD_PROFILES: [&str; 2] = ["release", "debug"];
const PATH_VAR: &str = "PATH";
const SLUG_ESCAPE: char = '_';
const SCRIPT_SHEBANG: &str = "#!/bin/sh";
const BUILD_HOST_CLI: &str = "just agents-linux";

pub(in crate::environment_shell) struct RemoteBus {
    forward: RemoteForward,
    bin_dir: PathBuf,
    token_file: PathBuf,
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

#[derive(Debug)]
pub(in crate::environment_shell) enum RemoteBusError {
    NoBundledCli(HostPlatform),
    Host(HostError),
    Bundle(std::io::Error),
}

impl fmt::Display for RemoteBusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBundledCli(platform) => write!(
                f,
                "this build has no {CLI_PROGRAM} for {platform} hosts; run `{BUILD_HOST_CLI}` and reconnect"
            ),
            Self::Host(error) => error.fmt(f),
            Self::Bundle(error) => write!(f, "could not read the bundled {CLI_PROGRAM}: {error}"),
        }
    }
}

impl From<HostError> for RemoteBusError {
    fn from(error: HostError) -> Self {
        Self::Host(error)
    }
}

impl From<std::io::Error> for RemoteBusError {
    fn from(error: std::io::Error) -> Self {
        Self::Bundle(error)
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

    pub(in crate::environment_shell) fn prepare(self) -> Result<RemoteBus, RemoteBusError> {
        let platform = self
            .ssh
            .platform()
            .ok_or(RemoteBusError::Host(HostError::NotConnected))?;
        let layout = self
            .ssh
            .layout()
            .ok_or(RemoteBusError::Host(HostError::NotConnected))?;
        let bin_dir = layout.bin_dir();
        let cli = bin_dir.join(CLI_PROGRAM);
        if !self.has_current_cli(&cli) {
            let bundled = bundled_cli(platform).ok_or(RemoteBusError::NoBundledCli(platform))?;
            self.link
                .create_dir_with_mode(&bin_dir, FileMode::PRIVATE_DIR)?;
            self.link.write(&cli, &std::fs::read(bundled)?)?;
            self.link.set_permissions(&cli, FileMode::EXECUTABLE)?;
        }
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
        Ok(RemoteBus {
            forward,
            bin_dir,
            token_file,
            claude_settings,
        })
    }

    fn has_current_cli(&self, cli: &Path) -> bool {
        let mut version = HostCommand::new(cli);
        version.arg(VERSION_FLAG);
        self.link.output(&version).is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == HOST_CLI_BUILD
        })
    }
}

impl RemoteBus {
    pub(in crate::environment_shell) fn caller_vars(
        &self,
        caller: &Caller,
    ) -> Vec<(String, String)> {
        let address = format!("{LOOPBACK}:{}", self.forward.remote_port);
        let mut vars = token_file_env(&address, &self.token_file, caller);
        vars.push((
            PATH_VAR.to_owned(),
            self.bin_dir.to_string_lossy().into_owned(),
        ));
        vars
    }

    pub(in crate::environment_shell) fn askpass(
        &self,
        caller: &Caller,
    ) -> (PathBuf, Vec<(String, String)>) {
        let address = format!("{LOOPBACK}:{}", self.forward.remote_port);
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

pub(in crate::environment_shell) fn bundled_cli(platform: HostPlatform) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    bundle_candidates(exe_dir, platform)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn bundle_candidates(exe_dir: &Path, platform: HostPlatform) -> Vec<PathBuf> {
    let platform_dir = platform.to_string();
    let mut resources = exe_dir.to_path_buf();
    resources.extend(MACOS_RESOURCES);
    let mut candidates = vec![
        exe_dir
            .join(BUNDLE_AGENTS_DIR)
            .join(&platform_dir)
            .join(CLI_PROGRAM),
        resources
            .join(BUNDLE_AGENTS_DIR)
            .join(&platform_dir)
            .join(CLI_PROGRAM),
    ];
    if let Some(target_dir) = exe_dir.parent() {
        candidates.extend(BUILD_PROFILES.iter().map(|profile| {
            target_dir
                .join(platform.musl_target())
                .join(profile)
                .join(CLI_PROGRAM)
        }));
    }
    candidates
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
    fn the_cli_is_looked_for_in_the_bundle_then_in_the_cross_builds() {
        let candidates =
            bundle_candidates(Path::new("/repo/target/debug"), HostPlatform::LinuxAarch64);
        assert_eq!(
            candidates,
            [
                PathBuf::from("/repo/target/debug/agents/linux-aarch64/krad-ade-agents"),
                PathBuf::from(
                    "/repo/target/debug/../Resources/agents/linux-aarch64/krad-ade-agents"
                ),
                PathBuf::from("/repo/target/aarch64-unknown-linux-musl/release/krad-ade-agents"),
                PathBuf::from("/repo/target/aarch64-unknown-linux-musl/debug/krad-ade-agents"),
            ]
        );
    }
}
