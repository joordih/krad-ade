use std::ffi::OsString;

use super::{option_pair, platform, SshArgs, BATCH_MODE_ON};

pub const LOOPBACK: &str = "127.0.0.1";
const LOCAL_FORWARD_FLAG: &str = "-L";
const REMOTE_FORWARD_FLAG: &str = "-R";
const NO_COMMAND_FLAG: &str = "-N";
const EXIT_ON_FAILURE: &str = "ExitOnForwardFailure=yes";
const ANNOUNCE_ALLOCATED_PORTS: &str = "LogLevel=INFO";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Local,
    Remote,
}

impl Direction {
    pub(crate) const fn flag(self) -> &'static str {
        match self {
            Self::Local => LOCAL_FORWARD_FLAG,
            Self::Remote => REMOTE_FORWARD_FLAG,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForwardRequest {
    Open,
    Cancel,
}

pub(crate) fn local_spec(local_port: u16, remote_port: u16) -> String {
    format!("{LOOPBACK}:{local_port}:{LOOPBACK}:{remote_port}")
}

pub(crate) fn remote_spec(remote_port: u16, local_port: u16) -> String {
    format!("{LOOPBACK}:{remote_port}:{LOOPBACK}:{local_port}")
}

impl SshArgs {
    pub(crate) fn mux_forward_args(
        &self,
        direction: Direction,
        spec: &str,
        request: ForwardRequest,
    ) -> Option<Vec<OsString>> {
        platform::mux_forward_args(self, direction, spec, request)
    }

    pub(crate) fn tunnel_args(&self, direction: Direction, spec: &str) -> Vec<OsString> {
        let mut args = self.options();
        args.extend(platform::client_options(self.control_dir()));
        let mut options = vec![BATCH_MODE_ON, EXIT_ON_FAILURE];
        if direction == Direction::Remote {
            options.push(ANNOUNCE_ALLOCATED_PORTS);
        }
        for option in options {
            args.extend(option_pair(option.to_owned()));
        }
        args.push(OsString::from(NO_COMMAND_FLAG));
        args.push(OsString::from(direction.flag()));
        args.push(OsString::from(spec));
        args.extend(self.destination_args());
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_local_forward_binds_loopback_on_both_ends() {
        assert_eq!(local_spec(5000, 41_234), "127.0.0.1:5000:127.0.0.1:41234");
    }

    #[test]
    fn a_remote_tunnel_announces_the_port_it_was_given() -> TestResult {
        assert_eq!(remote_spec(0, 5000), "127.0.0.1:0:127.0.0.1:5000");
        let args = SshArgs::new("devbox".parse()?, None);
        let tunnel = strings(&args.tunnel_args(Direction::Remote, &remote_spec(0, 5000)));
        assert!(tunnel
            .windows(2)
            .any(|pair| pair == ["-R", "127.0.0.1:0:127.0.0.1:5000"]));
        assert!(tunnel
            .windows(2)
            .any(|pair| pair == ["-o", "LogLevel=INFO"]));
        Ok(())
    }

    #[test]
    fn a_tunnel_runs_no_command_and_fails_when_the_forward_does() -> TestResult {
        let args = SshArgs::new("me@devbox:2200".parse()?, None);
        let tunnel = strings(&args.tunnel_args(Direction::Local, &local_spec(1, 2)));
        assert!(tunnel.contains(&"-N".to_owned()));
        assert!(tunnel
            .windows(2)
            .any(|pair| pair == ["-o", "ExitOnForwardFailure=yes"]));
        assert!(tunnel
            .windows(2)
            .any(|pair| pair == ["-L", "127.0.0.1:1:127.0.0.1:2"]));
        assert_eq!(tunnel[tunnel.len() - 2..], ["--", "me@devbox"]);
        Ok(())
    }
}
