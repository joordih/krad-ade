use krad_domain::{AcpPackageState, NpmPackage};
use krad_host::{local_output_within, HostError};
use serde::Deserialize;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::Output;
use std::time::Duration;
use thiserror::Error;

use crate::process::{harness_command, Runner};

const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const INSTALL_TIMEOUT: Duration = Duration::from_mins(10);
const LIST_ARGS: [&str; 4] = ["ls", "--global", "--depth=0", "--json"];
const INSTALL_ARGS: [&str; 4] = ["install", "--global", "--no-audit", "--no-fund"];
const ERROR_LINES: usize = 3;
const NO_OUTPUT_TEXT: &str = "npm stopped without saying why";

#[derive(Debug, Error)]
pub enum AcpPackageError {
    #[error("cannot run npm: {0}")]
    Spawn(#[from] io::Error),
    #[error("npm did not finish: {0}")]
    Run(#[from] HostError),
    #[error("npm could not install {package}: {reason}")]
    Install { package: String, reason: String },
}

#[derive(Debug, Default, Deserialize)]
struct GlobalList {
    #[serde(default)]
    dependencies: HashMap<String, Installed>,
}

#[derive(Debug, Deserialize)]
struct Installed {
    version: Option<String>,
}

fn installed_state(stdout: &[u8], package: NpmPackage) -> AcpPackageState {
    let list: GlobalList = serde_json::from_slice(stdout).unwrap_or_default();
    list.dependencies
        .get(package.name)
        .map_or(AcpPackageState::Missing, |installed| {
            AcpPackageState::Ready {
                version: installed.version.clone(),
            }
        })
}

pub(super) fn failure_reason(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = lines[lines.len().saturating_sub(ERROR_LINES)..].join(" ");
    if tail.is_empty() {
        NO_OUTPUT_TEXT.to_owned()
    } else {
        tail
    }
}

fn run_npm(
    npm: &Runner,
    cwd: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<Output, AcpPackageError> {
    let command = harness_command(npm, cwd, args, &[])?;
    Ok(local_output_within(command, timeout)?)
}

fn with_package(args: &[&str], package: String) -> Vec<String> {
    args.iter()
        .map(|&arg| arg.to_owned())
        .chain(std::iter::once(package))
        .collect()
}

/// # Errors
///
/// Fails when npm cannot be run or does not answer in time.
#[tracing::instrument(skip_all, fields(package = package.name))]
pub fn acp_package_state(
    npm: &Runner,
    cwd: &Path,
    package: NpmPackage,
) -> Result<AcpPackageState, AcpPackageError> {
    let output = run_npm(
        npm,
        cwd,
        &with_package(&LIST_ARGS, package.name.to_owned()),
        CHECK_TIMEOUT,
    )?;
    Ok(installed_state(&output.stdout, package))
}

/// # Errors
///
/// Fails with npm's own reason when the package cannot be installed.
#[tracing::instrument(skip_all, fields(package = package.name))]
pub fn install_acp_package(
    npm: &Runner,
    cwd: &Path,
    package: NpmPackage,
) -> Result<AcpPackageState, AcpPackageError> {
    let output = run_npm(
        npm,
        cwd,
        &with_package(&INSTALL_ARGS, package.latest()),
        INSTALL_TIMEOUT,
    )?;
    if !output.status.success() {
        return Err(AcpPackageError::Install {
            package: package.name.to_owned(),
            reason: failure_reason(&output),
        });
    }
    tracing::info!("installed an ACP agent package");
    Ok(
        acp_package_state(npm, cwd, package).unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read the installed package's version");
            AcpPackageState::Ready { version: None }
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADAPTER: NpmPackage = NpmPackage {
        name: "@agentclientprotocol/claude-agent-acp",
        bin: "claude-agent-acp",
    };

    #[test]
    fn the_global_list_tells_the_installed_version() {
        let listed = br#"{"name": "lib", "dependencies": {
            "@agentclientprotocol/claude-agent-acp": {"version": "0.106.0", "overridden": false},
            "npm": {"version": "11.0.0"}}}"#;
        assert_eq!(
            installed_state(listed, ADAPTER),
            AcpPackageState::Ready {
                version: Some("0.106.0".to_owned())
            }
        );
    }

    #[test]
    fn a_package_missing_from_the_global_list_is_not_installed() {
        assert_eq!(
            installed_state(br#"{"name": "lib"}"#, ADAPTER),
            AcpPackageState::Missing
        );
        assert_eq!(installed_state(b"", ADAPTER), AcpPackageState::Missing);
    }

    #[test]
    fn a_failed_install_keeps_the_last_lines_npm_wrote() {
        let output = |stderr: &[u8]| Output {
            status: std::process::ExitStatus::default(),
            stdout: Vec::new(),
            stderr: stderr.to_vec(),
        };
        let failed = output(b"npm warn deprecated x\n\nnpm error code EACCES\nnpm error syscall mkdir\nnpm error path /usr/lib/node_modules\n");
        assert_eq!(
            failure_reason(&failed),
            "npm error code EACCES npm error syscall mkdir npm error path /usr/lib/node_modules"
        );
        assert_eq!(failure_reason(&output(b"")), NO_OUTPUT_TEXT);
    }

    #[test]
    fn the_install_asks_for_the_latest_release_globally() {
        assert_eq!(
            with_package(&INSTALL_ARGS, ADAPTER.latest()),
            [
                "install",
                "--global",
                "--no-audit",
                "--no-fund",
                "@agentclientprotocol/claude-agent-acp@latest"
            ]
        );
    }
}
