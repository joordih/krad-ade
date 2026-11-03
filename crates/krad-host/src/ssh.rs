mod forward;
mod platform;

pub use forward::LOOPBACK;
pub(crate) use forward::{local_spec, remote_spec, Direction, ForwardRequest};

use krad_domain::{HostTimeouts, SshDestination};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

const SSH_PROGRAM: &str = "ssh";
pub(crate) const OPTION_FLAG: &str = "-o";
pub(crate) const BATCH_MODE_ON: &str = "BatchMode=yes";
const PORT_FLAG: &str = "-p";
const TTY_FLAG: &str = "-t";
const NO_TTY_FLAG: &str = "-T";
const SUBSYSTEM_FLAG: &str = "-s";
const END_OF_OPTIONS: &str = "--";
const SFTP_SUBSYSTEM: &str = "sftp";
const ALIVE_INTERVAL: Duration = Duration::from_secs(15);
const ALIVE_COUNT_MAX: u32 = 3;
const ALIVE_INTERVAL_OPTION: &str = "ServerAliveInterval";
const ALIVE_COUNT_MAX_OPTION: &str = "ServerAliveCountMax";
const CONNECT_TIMEOUT_OPTION: &str = "ConnectTimeout";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    ControlMaster,
    Multiplexed,
}

impl Default for Transport {
    fn default() -> Self {
        platform::DEFAULT_TRANSPORT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Interaction {
    Batch,
    Terminal,
    Subsystem,
}

#[derive(Clone, Debug)]
pub struct SshArgs {
    pub destination: SshDestination,
    pub timeouts: HostTimeouts,
    pub transport: Transport,
    config_dir: Option<PathBuf>,
    control_dir: Arc<OnceLock<PathBuf>>,
}

impl PartialEq for SshArgs {
    fn eq(&self, other: &Self) -> bool {
        self.destination == other.destination && self.config_dir == other.config_dir
    }
}

impl Eq for SshArgs {}

impl SshArgs {
    #[must_use]
    pub fn new(destination: SshDestination, config_dir: Option<&Path>) -> Self {
        Self {
            destination,
            timeouts: HostTimeouts::default(),
            transport: Transport::default(),
            config_dir: config_dir.map(Path::to_owned),
            control_dir: Arc::new(OnceLock::new()),
        }
    }

    #[must_use]
    pub const fn with_timeouts(mut self, timeouts: HostTimeouts) -> Self {
        self.timeouts = timeouts;
        self
    }

    #[must_use]
    pub const fn with_transport(mut self, transport: Transport) -> Self {
        self.transport = transport;
        self
    }

    pub(crate) fn multiplexed_args(&self, word: OsString) -> Vec<OsString> {
        let mut args = self.options();
        args.push(OsString::from(NO_TTY_FLAG));
        args.extend(self.destination_args());
        args.push(word);
        args
    }

    pub(crate) fn program() -> OsString {
        OsString::from(SSH_PROGRAM)
    }

    pub(crate) fn options(&self) -> Vec<OsString> {
        let options = [
            format!("{ALIVE_INTERVAL_OPTION}={}", ALIVE_INTERVAL.as_secs()),
            format!("{ALIVE_COUNT_MAX_OPTION}={ALIVE_COUNT_MAX}"),
            format!(
                "{CONNECT_TIMEOUT_OPTION}={}",
                self.timeouts.connect.duration().as_secs()
            ),
        ];
        options.into_iter().flat_map(option_pair).collect()
    }

    pub(crate) fn control_dir(&self) -> Option<&Path> {
        self.control_dir.get().map(PathBuf::as_path)
    }

    pub(crate) fn destination_args(&self) -> Vec<OsString> {
        let mut args = Vec::new();
        if let Some(port) = self.destination.port {
            args.push(OsString::from(PORT_FLAG));
            args.push(OsString::from(port.to_string()));
        }
        args.push(OsString::from(END_OF_OPTIONS));
        args.push(OsString::from(self.destination.target()));
        args
    }

    pub(crate) fn base(&self, interaction: Interaction) -> Vec<OsString> {
        let mut args = self.options();
        args.extend(platform::client_options(self.control_dir()));
        match interaction {
            Interaction::Batch => {
                args.push(OsString::from(NO_TTY_FLAG));
                args.extend(option_pair(BATCH_MODE_ON.to_owned()));
            }
            Interaction::Terminal => args.push(OsString::from(TTY_FLAG)),
            Interaction::Subsystem => {
                args.extend(option_pair(BATCH_MODE_ON.to_owned()));
                args.push(OsString::from(SUBSYSTEM_FLAG));
            }
        }
        args.extend(self.destination_args());
        if interaction == Interaction::Subsystem {
            args.push(OsString::from(SFTP_SUBSYSTEM));
        }
        args
    }

    pub(crate) fn sign_in_args(&self, interaction: Interaction) -> Vec<OsString> {
        platform::sign_in_args(self, interaction)
    }

    pub(crate) fn check_args(&self) -> Option<Vec<OsString>> {
        platform::check_args(self)
    }

    pub(crate) fn sign_out_args(&self) -> Option<Vec<OsString>> {
        platform::sign_out_args(self)
    }

    pub(crate) fn ensure_control_dir(&self) -> std::io::Result<()> {
        if self.control_dir.get().is_some() {
            return Ok(());
        }
        let Some(config_dir) = &self.config_dir else {
            return Ok(());
        };
        if let Some(dir) = platform::prepare_control_dir(config_dir, self.transport)? {
            self.control_dir.get_or_init(|| dir);
        }
        Ok(())
    }
}

pub(crate) fn option_pair(option: String) -> [OsString; 2] {
    [OsString::from(OPTION_FLAG), OsString::from(option)]
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn args(destination: &str) -> Result<SshArgs, krad_domain::SshDestinationError> {
        Ok(SshArgs::new(destination.parse()?, None))
    }

    fn strings(args: Vec<OsString>) -> Vec<String> {
        args.into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn batch_commands_never_prompt_and_end_options_before_the_target() -> TestResult {
        let base = strings(args("me@devbox:2200")?.base(Interaction::Batch));
        assert!(base.windows(2).any(|pair| pair == ["-o", "BatchMode=yes"]));
        assert!(base.contains(&"-T".to_owned()));
        assert_eq!(base[base.len() - 4..], ["-p", "2200", "--", "me@devbox"]);
        Ok(())
    }

    #[test]
    fn every_connection_gives_up_after_the_connect_timeout() -> TestResult {
        let timeouts = HostTimeouts {
            connect: krad_domain::TimeoutSecs::from(7),
            ..HostTimeouts::default()
        };
        let base = strings(
            args("devbox")?
                .with_timeouts(timeouts)
                .base(Interaction::Batch),
        );
        assert!(base
            .windows(2)
            .any(|pair| pair == ["-o", "ConnectTimeout=7"]));
        Ok(())
    }

    #[test]
    fn the_sftp_channel_asks_for_the_subsystem_after_the_target() -> TestResult {
        let base = strings(args("devbox")?.base(Interaction::Subsystem));
        assert_eq!(base[base.len() - 3..], ["--", "devbox", "sftp"]);
        assert!(base.contains(&"-s".to_owned()));
        Ok(())
    }

    #[test]
    fn terminals_get_a_tty_and_may_prompt() -> TestResult {
        let base = strings(args("devbox")?.base(Interaction::Terminal));
        assert!(base.contains(&"-t".to_owned()));
        assert!(!base.contains(&"BatchMode=yes".to_owned()));
        Ok(())
    }
}
