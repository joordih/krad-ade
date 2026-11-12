use std::path::Path;
use std::str::FromStr;

use super::commands::authorize;
use super::{RUNNER_VERB, STATUS_VERB};
use crate::command::{quote, HostCommand};
use krad_domain::Pairing;

pub const SERVICE_NAME: &str = "krad-cloud-runner.service";
pub const READY_MARKER: &str = "krad-cloud-runner: ready";
pub const UPDATE_PENDING_MARKER: &str = "krad-cloud-runner: update pending";
pub const LINGER_UNAVAILABLE_MARKER: &str = "krad-cloud-runner: linger unavailable";
const UNIT_DIR: &str = "$HOME/.config/systemd/user";
const RUNTIME_DIR_DEFAULT: &str =
    "export XDG_RUNTIME_DIR=\"${XDG_RUNTIME_DIR:-/run/user/$(id -u)}\"";
const RUNNING_STATUS_PATTERN: &str = "*\"\trunning\t\"*";
const UNIT_QUOTE: char = '"';
const UNIT_ESCAPES: [(char, &str); 3] = [('\\', "\\\\"), ('"', "\\\""), ('%', "%%")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceState {
    Ready,
    UpdatePending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linger {
    Enabled,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstallReport {
    pub service: ServiceState,
    pub linger: Linger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the cloud runner's install script did not report its outcome")]
pub struct UnreportedInstall;

impl FromStr for InstallReport {
    type Err = UnreportedInstall;

    fn from_str(output: &str) -> Result<Self, Self::Err> {
        let has = |marker: &str| output.lines().any(|line| line.trim() == marker);
        if !has(READY_MARKER) {
            return Err(UnreportedInstall);
        }
        Ok(Self {
            service: if has(UPDATE_PENDING_MARKER) {
                ServiceState::UpdatePending
            } else {
                ServiceState::Ready
            },
            linger: if has(LINGER_UNAVAILABLE_MARKER) {
                Linger::Unavailable
            } else {
                Linger::Enabled
            },
        })
    }
}

fn unit_word(word: &str) -> String {
    let escaped = UNIT_ESCAPES
        .iter()
        .fold(word.to_owned(), |text, (raw, escaped)| {
            text.replace(*raw, escaped)
        });
    format!("{UNIT_QUOTE}{escaped}{UNIT_QUOTE}")
}

#[must_use]
pub fn service_unit(cli: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=krad-ade cloud runner\n\
         \n\
         [Service]\n\
         ExecStart={program} {RUNNER_VERB}\n\
         Restart=on-failure\n\
         KillMode=process\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        program = unit_word(&cli.to_string_lossy()),
    )
}

#[must_use]
pub fn install_script(cli: &Path) -> String {
    let unit = quote(&service_unit(cli));
    let cli = quote(&cli.to_string_lossy());
    format!(
        "set -u\n\
         {RUNTIME_DIR_DEFAULT}\n\
         service={SERVICE_NAME}\n\
         unit_dir=\"{UNIT_DIR}\"\n\
         unit=\"$unit_dir/$service\"\n\
         cli={cli}\n\
         runner_idle() {{\n\
         \x20 status=$(\"$cli\" {STATUS_VERB} 2>/dev/null) || return 1\n\
         \x20 case \"$status\" in {RUNNING_STATUS_PATTERN}) return 1 ;; esac\n\
         \x20 return 0\n\
         }}\n\
         mkdir -p \"$unit_dir\" || exit 1\n\
         printf '%s' {unit} > \"$unit.new\" || exit 1\n\
         changed=0\n\
         if [ -f \"$unit\" ] && ! cmp -s \"$unit\" \"$unit.new\"; then changed=1; fi\n\
         mv -f \"$unit.new\" \"$unit\" || exit 1\n\
         loginctl enable-linger \"$(id -un)\" >/dev/null 2>&1 || echo '{LINGER_UNAVAILABLE_MARKER}'\n\
         systemctl --user daemon-reload || exit 1\n\
         systemctl --user enable --now \"$service\" || exit 1\n\
         if [ \"$changed\" = 1 ]; then\n\
         \x20 if ! systemctl --user is-active --quiet \"$service\" || runner_idle; then\n\
         \x20   systemctl --user restart \"$service\" || exit 1\n\
         \x20 else\n\
         \x20   echo '{UPDATE_PENDING_MARKER}'\n\
         \x20 fi\n\
         fi\n\
         echo '{READY_MARKER}'\n"
    )
}

#[must_use]
pub fn uninstall_script() -> String {
    format!(
        "set -u\n\
         {RUNTIME_DIR_DEFAULT}\n\
         systemctl --user disable --now {SERVICE_NAME} >/dev/null 2>&1\n\
         rm -f \"{UNIT_DIR}/{SERVICE_NAME}\" || exit 1\n\
         systemctl --user daemon-reload >/dev/null 2>&1\n\
         exit 0\n"
    )
}

#[must_use]
pub fn install_command(cli: &Path, pairing: Option<&Pairing>) -> HostCommand {
    let mut command = HostCommand::posix_shell(&install_script(cli));
    if let Some(pairing) = pairing {
        authorize(&mut command, pairing);
    }
    command
}

#[must_use]
pub fn uninstall_command() -> HostCommand {
    HostCommand::posix_shell(&uninstall_script())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_keeps_detached_processes_alive_across_daemon_restarts() {
        let unit = service_unit(Path::new(
            "/home/me/.krad-ade/0.5.0+mux2/bin/krad-ade-agents",
        ));
        assert!(unit.contains(
            "\nExecStart=\"/home/me/.krad-ade/0.5.0+mux2/bin/krad-ade-agents\" cloud-runner\n"
        ));
        assert!(unit.contains("\nRestart=on-failure\n"));
        assert!(unit.contains("\nKillMode=process\n"));
        assert!(unit.contains("\nWantedBy=default.target\n"));
    }

    #[test]
    fn the_unit_escapes_quotes_backslashes_and_specifiers() {
        let unit = service_unit(Path::new("/home/a \"b\"/100%/x\\y"));
        assert!(unit.contains("ExecStart=\"/home/a \\\"b\\\"/100%%/x\\\\y\" cloud-runner"));
    }

    #[test]
    fn the_install_script_enables_lingering_and_restarts_only_when_idle() {
        let script = install_script(Path::new("/opt/it's/krad-ade-agents"));
        assert!(script.contains("cli='/opt/it'\\''s/krad-ade-agents'\n"));
        assert!(script.contains("unit_dir=\"$HOME/.config/systemd/user\"\n"));
        assert!(script.contains(&format!(
            "loginctl enable-linger \"$(id -un)\" >/dev/null 2>&1 || echo '{LINGER_UNAVAILABLE_MARKER}'"
        )));
        assert!(script.contains("systemctl --user daemon-reload || exit 1\n"));
        assert!(script.contains("systemctl --user enable --now \"$service\" || exit 1\n"));
        assert!(script.contains("! cmp -s \"$unit\" \"$unit.new\"; then changed=1;"));
        assert!(script.contains("status=$(\"$cli\" cloud-status 2>/dev/null) || return 1"));
        assert!(script.contains("*\"\trunning\t\"*) return 1"));
        assert!(script.contains("systemctl --user restart \"$service\""));
        assert!(script.contains(&format!("echo '{UPDATE_PENDING_MARKER}'")));
        assert!(script.ends_with(&format!("echo '{READY_MARKER}'\n")));
        assert!(script.contains("KillMode=process"));
    }

    #[test]
    fn the_install_command_reads_the_token_from_stdin() -> Result<(), Box<dyn std::error::Error>> {
        let pairing = Pairing {
            install: "0123456789abcdef".parse()?,
            token: "00112233445566778899aabbccddeeff".parse()?,
        };
        let command = install_command(Path::new("/bin/krad-ade-agents"), Some(&pairing));
        assert!(!command.posix_script().contains(pairing.token.expose()));
        assert_eq!(command.env.len(), 1);
        assert_eq!(
            command.env_from_stdin,
            [std::ffi::OsString::from(super::super::TOKEN_ENV)]
        );
        let anonymous = install_command(Path::new("/bin/x"), None);
        assert_eq!(anonymous.env, []);
        assert_eq!(anonymous.env_from_stdin, Vec::<std::ffi::OsString>::new());
        Ok(())
    }

    #[test]
    fn the_uninstall_script_stops_and_removes_the_unit() {
        let script = uninstall_script();
        assert!(script.contains("systemctl --user disable --now krad-cloud-runner.service"));
        assert!(script.contains("rm -f \"$HOME/.config/systemd/user/krad-cloud-runner.service\""));
        assert!(script.contains("systemctl --user daemon-reload"));
    }

    #[test]
    fn the_install_report_reads_the_markers() {
        let ready: Result<InstallReport, _> = format!("{READY_MARKER}\n").parse();
        assert_eq!(
            ready,
            Ok(InstallReport {
                service: ServiceState::Ready,
                linger: Linger::Enabled,
            })
        );
        let pending: Result<InstallReport, _> =
            format!("{LINGER_UNAVAILABLE_MARKER}\n{UPDATE_PENDING_MARKER}\n{READY_MARKER}\n")
                .parse();
        assert_eq!(
            pending,
            Ok(InstallReport {
                service: ServiceState::UpdatePending,
                linger: Linger::Unavailable,
            })
        );
        assert!("".parse::<InstallReport>().is_err());
    }
}
