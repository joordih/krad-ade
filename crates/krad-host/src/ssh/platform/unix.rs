mod socket_dir;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::super::{
    option_pair, Direction, ForwardRequest, Interaction, SshArgs, Transport, BATCH_MODE_ON,
};

pub(crate) const DEFAULT_TRANSPORT: Transport = Transport::ControlMaster;

const MASTER_FLAG: &str = "-M";
const NO_COMMAND_FLAG: &str = "-N";
const BACKGROUND_FLAG: &str = "-f";
const CONTROL_COMMAND_FLAG: &str = "-O";
const EXIT_COMMAND: &str = "exit";
const CHECK_COMMAND: &str = "check";
const FORWARD_COMMAND: &str = "forward";
const CANCEL_COMMAND: &str = "cancel";
const CONNECTION_HASH_TOKEN: &str = "%C";
const CONTROL_MASTER_OFF: &str = "ControlMaster=no";
const PERSIST_OPTION: &str = "ControlPersist";
const CONTROL_PATH_OPTION: &str = "ControlPath";
const PERSIST_AFTER_LAST_CLIENT: Duration = Duration::from_mins(10);

fn control_path(dir: &Path) -> String {
    format!(
        "{CONTROL_PATH_OPTION}=\"{}\"",
        dir.join(CONNECTION_HASH_TOKEN).to_string_lossy()
    )
}

pub(crate) fn client_options(control_dir: Option<&Path>) -> Vec<OsString> {
    control_dir.map_or_else(Vec::new, |dir| {
        [control_path(dir), CONTROL_MASTER_OFF.to_owned()]
            .into_iter()
            .flat_map(option_pair)
            .collect()
    })
}

pub(crate) fn sign_in_args(args: &SshArgs, interaction: Interaction) -> Vec<OsString> {
    let mut built: Vec<OsString> = [MASTER_FLAG, NO_COMMAND_FLAG, BACKGROUND_FLAG]
        .into_iter()
        .map(OsString::from)
        .collect();
    if let Some(dir) = args.control_dir() {
        built.extend(option_pair(control_path(dir)));
        built.extend(option_pair(format!(
            "{PERSIST_OPTION}={}",
            PERSIST_AFTER_LAST_CLIENT.as_secs()
        )));
    }
    if interaction == Interaction::Batch {
        built.extend(option_pair(BATCH_MODE_ON.to_owned()));
    }
    built.extend(args.options());
    built.extend(args.destination_args());
    built
}

fn control_command(args: &SshArgs, command: &str) -> Option<Vec<OsString>> {
    let dir = args.control_dir()?;
    let mut built = option_pair(control_path(dir)).to_vec();
    built.push(OsString::from(CONTROL_COMMAND_FLAG));
    built.push(OsString::from(command));
    Some(built)
}

pub(crate) fn check_args(args: &SshArgs) -> Option<Vec<OsString>> {
    let mut built = control_command(args, CHECK_COMMAND)?;
    built.extend(args.destination_args());
    Some(built)
}

pub(crate) fn sign_out_args(args: &SshArgs) -> Option<Vec<OsString>> {
    let mut built = control_command(args, EXIT_COMMAND)?;
    built.extend(args.destination_args());
    Some(built)
}

pub(crate) fn mux_forward_args(
    args: &SshArgs,
    direction: Direction,
    spec: &str,
    request: ForwardRequest,
) -> Option<Vec<OsString>> {
    let command = match request {
        ForwardRequest::Open => FORWARD_COMMAND,
        ForwardRequest::Cancel => CANCEL_COMMAND,
    };
    let mut built = control_command(args, command)?;
    built.push(OsString::from(direction.flag()));
    built.push(OsString::from(spec));
    built.extend(args.destination_args());
    Some(built)
}

pub(crate) fn prepare_control_dir(
    config_dir: &Path,
    transport: Transport,
) -> std::io::Result<Option<PathBuf>> {
    match transport {
        Transport::ControlMaster => socket_dir::prepare(config_dir).map(Some),
        Transport::Multiplexed => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::local_spec;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_multiplexed_forward_is_opened_and_cancelled_through_the_master() -> TestResult {
        let base = std::env::temp_dir().join(format!("krad-mux-{}", std::process::id()));
        let args = SshArgs::new("devbox".parse()?, Some(&base));
        assert!(mux_forward_args(&args, Direction::Local, "x", ForwardRequest::Open).is_none());
        args.ensure_control_dir()?;
        let spec = local_spec(1, 2);
        let open = strings(
            &mux_forward_args(&args, Direction::Local, &spec, ForwardRequest::Open)
                .ok_or("no master")?,
        );
        assert!(open.windows(2).any(|pair| pair == ["-O", "forward"]));
        let cancel = strings(
            &mux_forward_args(&args, Direction::Local, &spec, ForwardRequest::Cancel)
                .ok_or("no master")?,
        );
        assert!(cancel.windows(2).any(|pair| pair == ["-O", "cancel"]));
        std::fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn signing_in_starts_a_persistent_master_in_the_background() -> TestResult {
        let base = std::env::temp_dir().join(format!("krad-master-{}", std::process::id()));
        let args = SshArgs::new("devbox".parse()?, Some(&base));
        args.ensure_control_dir()?;
        let batch = strings(&sign_in_args(&args, Interaction::Batch));
        assert_eq!(batch[..3], ["-M", "-N", "-f"]);
        assert!(batch.iter().any(|arg| arg.starts_with("ControlPersist=")));
        assert!(batch.contains(&"BatchMode=yes".to_owned()));
        let terminal = strings(&sign_in_args(&args, Interaction::Terminal));
        assert!(!terminal.contains(&"BatchMode=yes".to_owned()));
        std::fs::remove_dir_all(base)?;
        Ok(())
    }
}
