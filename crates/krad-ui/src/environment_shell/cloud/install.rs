use krad_domain::{ClaudeOauthToken, CloudRunnerId, InstallId, Pairing, PairingToken};
use krad_host::runner::{
    install_command, pair_command, run_authorized, status_command, uninstall_command,
    unpair_command, InstallReport, UnreportedInstall,
};
use krad_host::{HostCommand, HostError, HostLink, RemoteLink};
use krad_store::{
    find_cloud_claude_token, find_cloud_runner_token, write_cloud_runner_token, LoginStoreError,
};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Output;

use super::super::host_cli::{upload_host_cli, HostCliError};

const INSTALL_SCRIPT: &str = "the cloud runner's install script";
const NO_INSTALL_ID: &str = "this installation has no id, so no cloud runner can pair with it";
const BAD_TOKEN: &str = "the cloud runner answered the pairing with an invalid token";

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct PreparedRunner {
    pub(in crate::environment_shell) cli: PathBuf,
    pub(in crate::environment_shell) pairing: Pairing,
    pub(in crate::environment_shell) report: InstallReport,
    pub(in crate::environment_shell) claude_token: Option<ClaudeOauthToken>,
}

#[derive(Debug)]
pub(in crate::environment_shell) enum RunnerSetupError {
    NoInstallId,
    Cli(HostCliError),
    Host(HostError),
    Vault(LoginStoreError),
    BadToken,
    Install(UnreportedInstall),
}

impl fmt::Display for RunnerSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInstallId => f.write_str(NO_INSTALL_ID),
            Self::Cli(error) => error.fmt(f),
            Self::Host(error) => error.fmt(f),
            Self::Vault(error) => error.fmt(f),
            Self::BadToken => f.write_str(BAD_TOKEN),
            Self::Install(error) => error.fmt(f),
        }
    }
}

impl From<HostCliError> for RunnerSetupError {
    fn from(error: HostCliError) -> Self {
        Self::Cli(error)
    }
}

impl From<HostError> for RunnerSetupError {
    fn from(error: HostError) -> Self {
        Self::Host(error)
    }
}

impl From<LoginStoreError> for RunnerSetupError {
    fn from(error: LoginStoreError) -> Self {
        Self::Vault(error)
    }
}

impl From<UnreportedInstall> for RunnerSetupError {
    fn from(error: UnreportedInstall) -> Self {
        Self::Install(error)
    }
}

fn succeeded(output: Output, program: &str) -> Result<Output, HostError> {
    if output.status.success() {
        return Ok(output);
    }
    Err(HostError::Remote {
        program: program.to_owned(),
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

fn pair_anew(
    link: &HostLink,
    cli: &Path,
    runner: CloudRunnerId,
    install: InstallId,
) -> Result<Pairing, RunnerSetupError> {
    let command = pair_command(cli, &install);
    let output = succeeded(link.output(&command)?, &command.program_name())?;
    let token: PairingToken = String::from_utf8_lossy(&output.stdout)
        .parse()
        .map_err(|_| RunnerSetupError::BadToken)?;
    write_cloud_runner_token(runner, &token)?;
    tracing::info!(%runner, "paired with the cloud runner");
    Ok(Pairing { install, token })
}

fn install(
    link: &HostLink,
    cli: &Path,
    pairing: &Pairing,
) -> Result<InstallReport, RunnerSetupError> {
    let output = succeeded(
        run_authorized(link, &install_command(cli, Some(pairing)), pairing)?,
        INSTALL_SCRIPT,
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).parse()?)
}

fn kept_claude_token(runner: CloudRunnerId) -> Option<ClaudeOauthToken> {
    find_cloud_claude_token(runner)
        .inspect_err(
            |error| tracing::warn!(%runner, %error, "the Claude Code token could not be read"),
        )
        .ok()
        .flatten()
        .map(|(token, _)| token)
}

fn is_authorized(link: &HostLink, cli: &Path, pairing: &Pairing) -> bool {
    run_authorized(link, &status_command(cli, pairing), pairing)
        .is_ok_and(|output| output.status.success())
}

#[tracing::instrument(skip_all, fields(%runner))]
pub(in crate::environment_shell) fn prepare_runner(
    link: &HostLink,
    ssh: &RemoteLink,
    runner: CloudRunnerId,
    install_id: Option<InstallId>,
) -> Result<PreparedRunner, RunnerSetupError> {
    let install_id = install_id.ok_or(RunnerSetupError::NoInstallId)?;
    let platform = ssh.platform().ok_or(HostError::NotConnected)?;
    let layout = ssh.layout().ok_or(HostError::NotConnected)?;
    let cli = upload_host_cli(link, platform, &layout.bin_dir())?;
    let kept = find_cloud_runner_token(runner)?;
    let Some((token, _)) = kept else {
        let pairing = pair_anew(link, &cli, runner, install_id)?;
        let report = install(link, &cli, &pairing)?;
        return Ok(PreparedRunner {
            cli,
            pairing,
            report,
            claude_token: kept_claude_token(runner),
        });
    };
    let pairing = Pairing {
        install: install_id,
        token,
    };
    let report = install(link, &cli, &pairing)?;
    if is_authorized(link, &cli, &pairing) {
        return Ok(PreparedRunner {
            cli,
            pairing,
            report,
            claude_token: kept_claude_token(runner),
        });
    }
    tracing::info!(%runner, "the cloud runner forgot this installation; pairing again");
    let pairing = pair_anew(link, &cli, runner, pairing.install)?;
    let report = install(link, &cli, &pairing)?;
    Ok(PreparedRunner {
        cli,
        pairing,
        report,
        claude_token: kept_claude_token(runner),
    })
}

fn run_checked(link: &HostLink, command: &HostCommand) -> Result<(), HostError> {
    succeeded(link.output(command)?, &command.program_name()).map(drop)
}

#[tracing::instrument(skip_all)]
pub(in crate::environment_shell) fn retire_runner(
    link: &HostLink,
    paired: Option<&(PathBuf, Pairing)>,
) -> Vec<HostError> {
    let unpaired =
        paired.map(|(cli, pairing)| run_checked(link, &unpair_command(cli, &pairing.install)));
    let uninstalled = Some(run_checked(link, &uninstall_command()));
    unpaired
        .into_iter()
        .chain(uninstalled)
        .filter_map(Result::err)
        .collect()
}
