use krad_host::runner::{report_verb_failure, runner_root, EXIT_OK};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::config::{
    read_web_access, turn_off_web_access, write_web_access, TlsFiles, WebAccess, WebAccessError,
};

pub const WEB_ACCESS_VERB: &str = "cloud-web-access";
const USAGE: &str = "usage: cloud-web-access status | off | on <address:port> [--certificate <file> --key <file>] [--name <host>]...";
const CERTIFICATE_FLAG: &str = "--certificate";
const KEY_FLAG: &str = "--key";
const NAME_FLAG: &str = "--name";
const OFF: &str = "off";

fn usage() -> WebAccessError {
    WebAccessError::Io(io::Error::new(io::ErrorKind::InvalidInput, USAGE))
}

fn parse_on(rest: &[String]) -> Result<WebAccess, WebAccessError> {
    let (bind, flags) = rest.split_first().ok_or_else(usage)?;
    let bind = bind.parse().map_err(|_| usage())?;
    let (mut certificate, mut key, mut names) = (None, None, Vec::new());
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        let value = flags.next().ok_or_else(usage)?;
        match flag.as_str() {
            CERTIFICATE_FLAG => certificate = Some(PathBuf::from(value)),
            KEY_FLAG => key = Some(PathBuf::from(value)),
            NAME_FLAG => names.push(value.to_ascii_lowercase()),
            _ => return Err(usage()),
        }
    }
    let tls = match (certificate, key) {
        (Some(certificate), Some(key)) => Some(TlsFiles { certificate, key }),
        (None, None) => None,
        _ => return Err(usage()),
    };
    Ok(WebAccess { bind, tls, names })
}

fn status(root: &Path) -> Result<String, WebAccessError> {
    Ok(match read_web_access(root)? {
        Some(access) => serde_json::to_string(&access)?,
        None => OFF.to_owned(),
    })
}

/// # Errors
///
/// Fails on a malformed command, refused settings or a settings file that cannot be written.
pub fn web_access_command(verb_args: &[String], root: &Path) -> Result<String, WebAccessError> {
    let (action, rest) = verb_args.split_first().ok_or_else(usage)?;
    match action.as_str() {
        "status" => status(root),
        "off" => {
            turn_off_web_access(root)?;
            Ok(OFF.to_owned())
        }
        "on" => {
            write_web_access(root, &parse_on(rest)?)?;
            status(root)
        }
        _ => Err(usage()),
    }
}

#[must_use]
pub fn run_web_access_verb(verb: &str, rest: &[String]) -> Option<i32> {
    if verb != WEB_ACCESS_VERB {
        return None;
    }
    let outcome = runner_root()
        .map_err(WebAccessError::Io)
        .and_then(|root| web_access_command(rest, &root))
        .and_then(|said| {
            let mut out = io::stdout().lock();
            writeln!(out, "{said}")?;
            out.flush()?;
            Ok(EXIT_OK)
        });
    Some(outcome.unwrap_or_else(|error| report_verb_failure(&error.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn on_reads_the_address_certificate_and_names() -> Result<(), Box<dyn std::error::Error>> {
        let access = parse_on(&words(
            "100.101.102.103:7421 --certificate m.crt --key m.key --name Machine.ts.net",
        ))?;
        assert_eq!(access.bind, "100.101.102.103:7421".parse()?);
        assert!(access.tls.is_some());
        assert_eq!(access.names, ["machine.ts.net"]);
        assert!(parse_on(&words("127.0.0.1:7421 --certificate m.crt")).is_err());
        Ok(())
    }

    #[test]
    fn turning_on_off_the_tailnet_without_a_certificate_writes_nothing(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("krad-web-verb-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        assert!(web_access_command(&words("on 100.101.102.103:7421"), &root).is_err());
        assert_eq!(web_access_command(&words("status"), &root)?, OFF);
        web_access_command(&words("on 127.0.0.1:7421"), &root)?;
        assert!(web_access_command(&words("status"), &root)?.contains("127.0.0.1:7421"));
        assert_eq!(web_access_command(&words("off"), &root)?, OFF);
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }
}
