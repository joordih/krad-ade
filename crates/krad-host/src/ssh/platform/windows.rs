use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::super::{
    option_pair, Direction, ForwardRequest, Interaction, SshArgs, Transport, BATCH_MODE_ON,
};

pub(crate) const DEFAULT_TRANSPORT: Transport = Transport::Multiplexed;

const SIGN_IN_CHECK: &str = "exit";

pub(crate) fn client_options(_control_dir: Option<&Path>) -> Vec<OsString> {
    Vec::new()
}

pub(crate) fn sign_in_args(args: &SshArgs, interaction: Interaction) -> Vec<OsString> {
    let mut built = args.options();
    if interaction == Interaction::Batch {
        built.extend(option_pair(BATCH_MODE_ON.to_owned()));
    }
    built.extend(args.destination_args());
    built.push(OsString::from(SIGN_IN_CHECK));
    built
}

pub(crate) fn check_args(_args: &SshArgs) -> Option<Vec<OsString>> {
    None
}

pub(crate) fn sign_out_args(_args: &SshArgs) -> Option<Vec<OsString>> {
    None
}

pub(crate) fn mux_forward_args(
    _args: &SshArgs,
    _direction: Direction,
    _spec: &str,
    _request: ForwardRequest,
) -> Option<Vec<OsString>> {
    None
}

const NO_CONTROL_MASTER: &str = "OpenSSH on Windows has no ControlMaster";

pub(crate) fn prepare_control_dir(
    _config_dir: &Path,
    transport: Transport,
) -> std::io::Result<Option<PathBuf>> {
    match transport {
        Transport::Multiplexed => Ok(None),
        Transport::ControlMaster => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            NO_CONTROL_MASTER,
        )),
    }
}
