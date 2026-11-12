mod attach_args;

use krad_domain::{InstallId, Pairing};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use self::attach_args::{parse_attach, parse_size, resolve_program, PATH_VAR};
use super::clients::{pair, unpair};
use super::{
    attach, list, resize, runner_socket, stop, RunnerDaemon, ATTACH_VERB, INSTALL_ENV, PAIR_VERB,
    RESIZE_VERB, RUNNER_VERB, STATUS_VERB, STOP_VERB, TOKEN_ENV, UNPAIR_VERB,
};
use crate::layout::HostLayout;

const EXIT_OK: i32 = 0;
const EXIT_FAILED: i32 = 1;
const NO_HOME: &str = "the cloud runner needs HOME to find its folder";
const NO_PAIRING: &str =
    "the cloud runner needs KRAD_ADE_CLOUD_INSTALL and KRAD_ADE_CLOUD_TOKEN in the environment";
const NOT_ON_PATH: &str = "the program to start is not on the PATH";
const ATTACH_USAGE: &str = "usage: cloud-attach <id> <offset> [--cwd DIR] [--terminal COLS ROWS] \
                            [--pass NAME]... [-- PROGRAM ARGS...]";
const ID_USAGE: &str = "usage: cloud-stop <id>";
const RESIZE_USAGE: &str = "usage: cloud-resize <id> <cols> <rows>";
const INSTALL_USAGE: &str = "usage: cloud-pair <install> | cloud-unpair <install>";

#[must_use]
pub fn run_runner_verb(verb: &str, rest: &[String]) -> Option<i32> {
    let outcome = match verb {
        RUNNER_VERB => runner_root().and_then(|root| serve(&root)),
        ATTACH_VERB => run_attach(rest),
        STOP_VERB => run_stop(rest),
        RESIZE_VERB => run_resize(rest),
        STATUS_VERB => run_status(),
        PAIR_VERB => run_pair(rest),
        UNPAIR_VERB => run_unpair(rest),
        _ => return None,
    };
    Some(match outcome {
        Ok(code) => code,
        Err(error) => report(&error.to_string()),
    })
}

fn runner_root() -> io::Result<PathBuf> {
    let home = std::env::home_dir().ok_or_else(|| io::Error::other(NO_HOME))?;
    Ok(HostLayout::new(&home, None).runner_dir())
}

fn socket() -> io::Result<PathBuf> {
    Ok(runner_socket(&runner_root()?))
}

fn serve(root: &Path) -> io::Result<i32> {
    let daemon = RunnerDaemon::new(root);
    let listener = daemon.bind()?;
    tracing::info!("cloud runner listening");
    daemon.serve(&listener);
    Ok(EXIT_OK)
}

fn usage(text: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, text)
}

fn pairing_from(lookup: impl Fn(&str) -> Option<String>) -> io::Result<Pairing> {
    let install = lookup(INSTALL_ENV).and_then(|raw| raw.parse().ok());
    let token = lookup(TOKEN_ENV).and_then(|raw| raw.parse().ok());
    match (install, token) {
        (Some(install), Some(token)) => Ok(Pairing { install, token }),
        _ => Err(io::Error::new(io::ErrorKind::PermissionDenied, NO_PAIRING)),
    }
}

fn env_pairing() -> io::Result<Pairing> {
    pairing_from(|name| std::env::var(name).ok())
}

fn run_attach(rest: &[String]) -> io::Result<i32> {
    let mut args =
        parse_attach(rest, |name| std::env::var(name).ok()).ok_or_else(|| usage(ATTACH_USAGE))?;
    if let Some(launch) = args.launch.as_mut() {
        let path = launch
            .env
            .iter()
            .find(|(name, _)| name == PATH_VAR)
            .map(|(_, value)| value.as_str());
        launch.program = resolve_program(&launch.program, path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, NOT_ON_PATH))?;
    }
    let code = attach(
        &socket()?,
        &env_pairing()?,
        args.id,
        args.from,
        args.launch,
        io::stdin(),
        &mut io::stdout(),
    )?;
    Ok(code.unwrap_or(EXIT_FAILED))
}

fn run_stop(rest: &[String]) -> io::Result<i32> {
    let id = rest
        .first()
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| usage(ID_USAGE))?;
    stop(&socket()?, &env_pairing()?, id)?;
    Ok(EXIT_OK)
}

fn run_resize(rest: &[String]) -> io::Result<i32> {
    let (id, size) = rest
        .split_first()
        .and_then(|(id, rest)| {
            let (size, rest) = parse_size(rest)?;
            rest.is_empty().then_some((id.parse().ok()?, size))
        })
        .ok_or_else(|| usage(RESIZE_USAGE))?;
    resize(&socket()?, &env_pairing()?, id, size)?;
    Ok(EXIT_OK)
}

fn run_status() -> io::Result<i32> {
    let statuses = list(&socket()?, &env_pairing()?)?;
    let mut out = io::stdout().lock();
    for status in statuses {
        writeln!(out, "{status}")?;
    }
    out.flush()?;
    Ok(EXIT_OK)
}

fn install_arg(rest: &[String]) -> io::Result<InstallId> {
    rest.first()
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| usage(INSTALL_USAGE))
}

fn run_pair(rest: &[String]) -> io::Result<i32> {
    let token = pair(&runner_root()?, &install_arg(rest)?)?;
    let mut out = io::stdout().lock();
    writeln!(out, "{}", token.expose())?;
    out.flush()?;
    Ok(EXIT_OK)
}

fn run_unpair(rest: &[String]) -> io::Result<i32> {
    unpair(&runner_root()?, &install_arg(rest)?)?;
    Ok(EXIT_OK)
}

fn report(message: &str) -> i32 {
    io::stderr()
        .write_all(format!("krad-ade-agents: {message}\n").as_bytes())
        .ok();
    EXIT_FAILED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pairing_comes_only_from_both_environment_variables() {
        let both = |name: &str| match name {
            INSTALL_ENV => Some("0123456789abcdef".to_owned()),
            TOKEN_ENV => Some("00112233445566778899aabbccddeeff".to_owned()),
            _ => None,
        };
        assert!(pairing_from(both).is_ok());
        let no_token = |name: &str| both(name).filter(|_| name == INSTALL_ENV);
        assert!(pairing_from(no_token).is_err());
        let bad_token = |name: &str| match name {
            TOKEN_ENV => Some("not-a-token".to_owned()),
            _ => both(name),
        };
        assert!(pairing_from(bad_token).is_err());
    }
}
