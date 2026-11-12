use krad_agents::CLI_PROGRAM;
use krad_host::{FileMode, HostCommand, HostError, HostLink, HostPlatform, HOST_CLI_BUILD};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

const VERSION_FLAG: &str = "--version";
const BUNDLE_AGENTS_DIR: &str = "agents";
const MACOS_RESOURCES: [&str; 2] = ["..", "Resources"];
const BUILD_PROFILES: [&str; 2] = ["release", "debug"];
const BUILD_HOST_CLI: &str = "just agents-linux";
const DIGEST_SUFFIX: &str = ".sha256";
const STAGED_EXTENSION: &str = "new";
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
const NIBBLE_BITS: u8 = 4;
const LOW_NIBBLE: u8 = 0x0f;
const DIGEST_LIMIT: u64 = 128;

#[derive(Debug)]
pub(in crate::environment_shell) enum HostCliError {
    NoBundledCli(HostPlatform),
    Host(HostError),
    Bundle(std::io::Error),
}

impl fmt::Display for HostCliError {
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

impl From<HostError> for HostCliError {
    fn from(error: HostError) -> Self {
        Self::Host(error)
    }
}

impl From<std::io::Error> for HostCliError {
    fn from(error: std::io::Error) -> Self {
        Self::Bundle(error)
    }
}

pub(in crate::environment_shell) fn upload_host_cli(
    link: &HostLink,
    platform: HostPlatform,
    bin_dir: &Path,
) -> Result<PathBuf, HostCliError> {
    let cli = bin_dir.join(CLI_PROGRAM);
    let digest_file = bin_dir.join(format!("{CLI_PROGRAM}{DIGEST_SUFFIX}"));
    match bundled_cli(platform) {
        Some(bundled) => {
            let binary = std::fs::read(bundled)?;
            let digest = digest_hex(&binary);
            if !has_cli_with_digest(link, &cli, &digest_file, &digest) {
                link.create_dir_with_mode(bin_dir, FileMode::PRIVATE_DIR)?;
                replace_cli(link, &cli, &binary)?;
                link.write(&digest_file, digest.as_bytes())?;
            }
        }
        None if has_current_cli(link, &cli) => {}
        None => return Err(HostCliError::NoBundledCli(platform)),
    }
    Ok(cli)
}

fn replace_cli(link: &HostLink, cli: &Path, binary: &[u8]) -> Result<(), HostError> {
    let staged = cli.with_extension(STAGED_EXTENSION);
    link.write(&staged, binary)?;
    link.set_permissions(&staged, FileMode::EXECUTABLE)?;
    if link.rename(&staged, cli).is_ok() {
        return Ok(());
    }
    if link.exists(cli) {
        link.remove_file(cli)?;
    }
    link.rename(&staged, cli)
}

fn has_cli_with_digest(link: &HostLink, cli: &Path, digest_file: &Path, digest: &str) -> bool {
    let uploaded = link.read(digest_file, DIGEST_LIMIT);
    uploaded.is_ok_and(|uploaded| uploaded == digest.as_bytes()) && has_current_cli(link, cli)
}

fn has_current_cli(link: &HostLink, cli: &Path) -> bool {
    let mut version = HostCommand::new(cli);
    version.arg(VERSION_FLAG);
    link.output(&version).is_ok_and(|output| {
        output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == HOST_CLI_BUILD
    })
}

fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| {
            [
                HEX_DIGITS[usize::from(byte >> NIBBLE_BITS)],
                HEX_DIGITS[usize::from(byte & LOW_NIBBLE)],
            ]
        })
        .map(char::from)
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
    fn a_cli_is_told_apart_by_the_digest_of_its_bytes() {
        assert_eq!(
            digest_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(digest_hex(b"build one"), digest_hex(b"build two"));
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
