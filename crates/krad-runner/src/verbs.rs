use krad_domain::{DeviceName, InstallId, PairingPermissions, PairingToken};
use krad_host::runner::{
    report_verb_failure, runner_root, EXIT_OK, PAIR_VERB, RUNNER_VERB, UNPAIR_VERB,
};
use std::io::{self, Write};
use std::path::Path;

use crate::pairings::{pair, unpair, SSH_DEVICE};
use crate::serving::{serve, Transport};

const INSTALL_USAGE: &str = "usage: cloud-pair <install> [device] | cloud-unpair <install>";

#[must_use]
pub fn run_runner_verb(verb: &str, rest: &[String]) -> Option<i32> {
    run_runner_verb_with(verb, rest, Vec::new())
}

#[must_use]
pub fn run_runner_verb_with(
    verb: &str,
    rest: &[String],
    transports: Vec<Transport>,
) -> Option<i32> {
    let outcome = match verb {
        RUNNER_VERB => runner_root().and_then(|root| serve(&root, transports)),
        PAIR_VERB => run_pair(rest),
        UNPAIR_VERB => run_unpair(rest),
        _ => return None,
    };
    Some(outcome.unwrap_or_else(|error| report_verb_failure(&error.to_string())))
}

fn usage() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, INSTALL_USAGE)
}

fn install_arg(rest: &[String]) -> io::Result<InstallId> {
    rest.first()
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(usage)
}

fn device_arg(rest: &[String]) -> io::Result<DeviceName> {
    rest.get(1)
        .map_or(SSH_DEVICE, String::as_str)
        .parse()
        .map_err(|_| usage())
}

/// # Errors
///
/// Fails when the pairing cannot be written to the runner's folder.
pub fn pair_over_ssh(
    root: &Path,
    install: &InstallId,
    device: DeviceName,
) -> io::Result<PairingToken> {
    pair(root, install, device, PairingPermissions::all())
}

fn run_pair(rest: &[String]) -> io::Result<i32> {
    let token = pair_over_ssh(&runner_root()?, &install_arg(rest)?, device_arg(rest)?)?;
    let mut out = io::stdout().lock();
    writeln!(out, "{}", token.expose())?;
    out.flush()?;
    Ok(EXIT_OK)
}

fn run_unpair(rest: &[String]) -> io::Result<i32> {
    unpair(&runner_root()?, &install_arg(rest)?)?;
    Ok(EXIT_OK)
}
