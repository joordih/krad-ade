use krad_domain::{CloudProcessId, InstallId, JournalOffset, Pairing};
use std::path::Path;
use std::process::Output;

use super::{
    ATTACH_VERB, INSTALL_ENV, PAIR_VERB, RESIZE_VERB, STATUS_VERB, STOP_VERB, TOKEN_ENV,
    UNPAIR_VERB,
};
use crate::command::HostCommand;
use crate::error::HostError;
use crate::link::HostLink;
use crate::path::remote_path;

const CWD_FLAG: &str = "--cwd";
const TERMINAL_FLAG: &str = "--terminal";
const PASS_FLAG: &str = "--pass";
const END_OF_FLAGS: &str = "--";

#[derive(Clone, Copy, Debug)]
pub struct TerminalAttach<'a> {
    pub id: &'a CloudProcessId,
    pub from: JournalOffset,
    pub cwd: &'a Path,
    pub cols: u16,
    pub rows: u16,
    pub passed: &'a [&'a str],
    pub program: &'a str,
    pub args: &'a [String],
}

pub fn authorize(command: &mut HostCommand, pairing: &Pairing) {
    command
        .env(INSTALL_ENV, pairing.install.as_str())
        .read_env_from_stdin(TOKEN_ENV);
}

/// # Errors
pub fn run_authorized(
    link: &HostLink,
    command: &HostCommand,
    pairing: &Pairing,
) -> Result<Output, HostError> {
    link.output_with_secret(command, pairing.token.expose())
}

#[must_use]
pub fn pair_command(cli: &Path, install: &InstallId) -> HostCommand {
    let mut command = HostCommand::new(cli);
    command.args([PAIR_VERB, install.as_str()]);
    command
}

#[must_use]
pub fn unpair_command(cli: &Path, install: &InstallId) -> HostCommand {
    let mut command = HostCommand::new(cli);
    command.args([UNPAIR_VERB, install.as_str()]);
    command
}

#[must_use]
pub fn status_command(cli: &Path, pairing: &Pairing) -> HostCommand {
    let mut command = HostCommand::new(cli);
    command.arg(STATUS_VERB);
    authorize(&mut command, pairing);
    command
}

#[must_use]
pub fn stop_command(cli: &Path, pairing: &Pairing, id: &CloudProcessId) -> HostCommand {
    let mut command = HostCommand::new(cli);
    command.args([STOP_VERB, id.as_str()]);
    authorize(&mut command, pairing);
    command
}

#[must_use]
pub fn resize_command(
    cli: &Path,
    pairing: &Pairing,
    id: &CloudProcessId,
    cols: u16,
    rows: u16,
) -> HostCommand {
    let mut command = HostCommand::new(cli);
    command.args([
        RESIZE_VERB,
        id.as_str(),
        &cols.to_string(),
        &rows.to_string(),
    ]);
    authorize(&mut command, pairing);
    command
}

#[must_use]
pub fn attach_terminal_command(
    cli: &Path,
    pairing: &Pairing,
    attach: &TerminalAttach<'_>,
) -> HostCommand {
    let mut command = HostCommand::new(cli);
    command
        .args([ATTACH_VERB, attach.id.as_str(), &attach.from.0.to_string()])
        .args([CWD_FLAG, &remote_path(attach.cwd)])
        .args([
            TERMINAL_FLAG,
            &attach.cols.to_string(),
            &attach.rows.to_string(),
        ])
        .in_login_shell();
    for name in attach.passed {
        command.args([PASS_FLAG, name]);
    }
    authorize(&mut command, pairing);
    command
        .arg(END_OF_FLAGS)
        .arg(attach.program)
        .args(attach.args);
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn pairing() -> Result<Pairing, Box<dyn std::error::Error>> {
        Ok(Pairing {
            install: "0123456789abcdef".parse()?,
            token: "00112233445566778899aabbccddeeff".parse()?,
        })
    }

    fn args(command: &HostCommand) -> Vec<OsString> {
        command.args.clone()
    }

    fn args_of(command: &HostCommand) -> Vec<String> {
        command
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn verbs_carry_the_install_in_the_environment_and_read_the_token_from_stdin() -> TestResult {
        let cli = Path::new("/home/me/.krad-ade/bin/krad-ade-agents");
        let pairing = pairing()?;
        let id: CloudProcessId = "shell-1".parse()?;
        let resize = resize_command(cli, &pairing, &id, 100, 30);
        assert_eq!(args(&resize), ["cloud-resize", "shell-1", "100", "30"]);
        assert_eq!(
            resize.env,
            [(
                OsString::from(INSTALL_ENV),
                OsString::from("0123456789abcdef")
            )]
        );
        assert_eq!(resize.env_from_stdin, [OsString::from(TOKEN_ENV)]);
        assert!(!resize.posix_script().contains(pairing.token.expose()));
        assert_eq!(
            args(&stop_command(cli, &pairing, &id)),
            ["cloud-stop", "shell-1"]
        );
        assert_eq!(args(&status_command(cli, &pairing)), ["cloud-status"]);
        assert_eq!(
            args(&pair_command(cli, &pairing.install)),
            ["cloud-pair", "0123456789abcdef"]
        );
        let unpair = unpair_command(cli, &pairing.install);
        assert_eq!(args(&unpair), ["cloud-unpair", "0123456789abcdef"]);
        assert_eq!(unpair.env, []);
        assert_eq!(unpair.env_from_stdin, Vec::<OsString>::new());
        Ok(())
    }

    #[test]
    fn a_terminal_attach_names_its_size_folder_and_program_but_never_the_token() -> TestResult {
        let cli = Path::new("/home/me/.krad-ade/bin/krad-ade-agents");
        let pairing = pairing()?;
        let id: CloudProcessId = "api-77-4".parse()?;
        let args = ["-l".to_owned()];
        let attach = TerminalAttach {
            id: &id,
            from: JournalOffset(0),
            cwd: Path::new("/srv/wt"),
            cols: 120,
            rows: 40,
            passed: &["TERM"],
            program: "bash",
            args: &args,
        };
        let command = attach_terminal_command(cli, &pairing, &attach);
        assert_eq!(
            args_of(&command),
            [
                "cloud-attach",
                "api-77-4",
                "0",
                "--cwd",
                "/srv/wt",
                "--terminal",
                "120",
                "40",
                "--pass",
                "TERM",
                "--",
                "bash",
                "-l"
            ]
        );
        assert_eq!(command.env_from_stdin, [OsString::from(TOKEN_ENV)]);
        assert!(command.through_login_shell);
        assert!(!command.posix_script().contains(pairing.token.expose()));
        Ok(())
    }
}
