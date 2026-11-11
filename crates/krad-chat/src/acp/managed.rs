use krad_domain::NpmPackage;
use krad_host::local_output_within;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::package::{failure_reason, AcpPackageError};
use crate::process::{harness_command, Runner};

const UPDATE_INTERVAL: Duration = Duration::from_hours(24);
const INSTALL_TIMEOUT: Duration = Duration::from_mins(10);
const MODULES_DIR: &str = "node_modules";
const BIN_DIR: &str = ".bin";
const PROGRAM_SUFFIXES: [&str; 3] = [".exe", ".cmd", ""];
const UPDATE_STAMP: &str = ".update-checked";
const PREFIX_FLAG: &str = "--prefix";
const INSTALL_ARGS: [&str; 3] = ["install", "--no-audit", "--no-fund"];

fn program_candidates(folder: &Path, package: NpmPackage) -> impl Iterator<Item = PathBuf> + '_ {
    let bin = folder.join(MODULES_DIR).join(BIN_DIR);
    PROGRAM_SUFFIXES
        .into_iter()
        .map(move |suffix| bin.join(format!("{}{suffix}", package.bin)))
}

#[must_use]
pub fn managed_adapter_program(folder: &Path, package: NpmPackage) -> Option<PathBuf> {
    program_candidates(folder, package).find(|candidate| candidate.is_file())
}

fn is_due(checked: Option<SystemTime>, now: SystemTime) -> bool {
    checked
        .and_then(|checked| now.duration_since(checked).ok())
        .is_none_or(|age| age >= UPDATE_INTERVAL)
}

#[must_use]
pub fn managed_adapter_update_due(folder: &Path) -> bool {
    let checked = fs::metadata(folder.join(UPDATE_STAMP))
        .and_then(|stamp| stamp.modified())
        .ok();
    is_due(checked, SystemTime::now())
}

fn stamp_update_check(folder: &Path) {
    if let Err(error) = fs::write(folder.join(UPDATE_STAMP), []) {
        tracing::warn!(%error, "could not note the adapter update check");
    }
}

fn install_args(folder: &Path, package: NpmPackage) -> Vec<String> {
    INSTALL_ARGS
        .iter()
        .map(|&arg| arg.to_owned())
        .chain([
            PREFIX_FLAG.to_owned(),
            folder.to_string_lossy().into_owned(),
            package.latest(),
        ])
        .collect()
}

/// # Errors
///
/// Fails when the folder cannot be made, npm cannot be run, or npm cannot
/// install the package.
#[tracing::instrument(skip_all, fields(package = package.name))]
pub fn install_managed_adapter(
    npm: &Runner,
    folder: &Path,
    package: NpmPackage,
) -> Result<PathBuf, AcpPackageError> {
    fs::create_dir_all(folder)?;
    stamp_update_check(folder);
    let command = harness_command(npm, folder, &install_args(folder, package), &[])?;
    let output = local_output_within(command, INSTALL_TIMEOUT)?;
    if !output.status.success() {
        return Err(AcpPackageError::Install {
            package: package.name.to_owned(),
            reason: failure_reason(&output),
        });
    }
    tracing::info!("installed an ACP adapter in the app's folder");
    managed_adapter_program(folder, package).ok_or_else(|| AcpPackageError::Install {
        package: package.name.to_owned(),
        reason: format!("npm installed no {} program", package.bin),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADAPTER: NpmPackage = NpmPackage {
        name: "@agentclientprotocol/claude-agent-acp",
        bin: "claude-agent-acp",
    };

    #[test]
    fn the_adapter_installs_into_the_app_folder_and_never_globally() {
        let folder = Path::new("adapters/claude-code-acp");
        let args = install_args(folder, ADAPTER);
        assert_eq!(
            args,
            [
                "install",
                "--no-audit",
                "--no-fund",
                "--prefix",
                &folder.to_string_lossy(),
                "@agentclientprotocol/claude-agent-acp@latest"
            ]
        );
        assert!(!args.iter().any(|arg| arg == "--global" || arg == "-g"));
    }

    #[test]
    fn the_program_is_looked_for_in_the_folder_s_own_bin() {
        let folder = Path::new("adapters");
        let candidates: Vec<PathBuf> = program_candidates(folder, ADAPTER).collect();
        let bin = folder.join(MODULES_DIR).join(BIN_DIR);
        assert_eq!(
            candidates,
            [
                bin.join("claude-agent-acp.exe"),
                bin.join("claude-agent-acp.cmd"),
                bin.join("claude-agent-acp"),
            ]
        );
        assert_eq!(managed_adapter_program(folder, ADAPTER), None);
    }

    #[test]
    fn an_update_is_due_once_a_day_or_when_never_checked() {
        let now = SystemTime::now();
        assert!(is_due(None, now));
        assert!(!is_due(Some(now), now));
        assert!(!is_due(now.checked_sub(Duration::from_hours(23)), now));
        assert!(is_due(now.checked_sub(UPDATE_INTERVAL), now));
    }
}
