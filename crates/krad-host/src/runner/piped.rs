use krad_domain::CloudProcessRun;
use std::fs::{self, File, OpenOptions};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::journal::Journal;
use super::process::{exit_code_of, group_alive, Tracked, SPAWN_FAILED};
use super::wire::CloudLaunch;

const SHELL: &str = "/bin/sh";
const SHELL_COMMAND_FLAG: &str = "-c";
const WRAPPER_NAME: &str = "krad-cloud-process";
const WRAPPER: &str = r#"input=$1 exit_file=$2 exit_temp=$3
shift 3
"$@" 0<>"$input"
code=$?
printf '%s\n' "$code" >"$exit_temp" && mv -f -- "$exit_temp" "$exit_file""#;
const FIFO_PROGRAM: &str = "mkfifo";
const FIFO_MODE_FLAG: &str = "-m";
const FIFO_MODE: &str = "600";
const END_OF_OPTIONS: &str = "--";
const OWN_PROCESS_GROUP: i32 = 0;
const FOLLOW_INTERVAL: Duration = Duration::from_millis(25);
const LIVENESS_INTERVAL: Duration = Duration::from_secs(1);
const FOLLOW_THREAD: &str = "krad-cloud-follow";

enum Watch {
    Child(Child),
    Adopted { group: u32, checked: Instant },
}

impl Watch {
    fn run(&mut self, journal: &Journal) -> CloudProcessRun {
        match self {
            Self::Child(child) => match child.try_wait() {
                Ok(None) => CloudProcessRun::Running,
                Ok(Some(status)) => {
                    CloudProcessRun::Ended(journal.exit_code().or_else(|| exit_code_of(status)))
                }
                Err(_) => CloudProcessRun::Ended(journal.exit_code()),
            },
            Self::Adopted { group, checked } => {
                if let Some(code) = journal.exit_code() {
                    return CloudProcessRun::Ended(Some(code));
                }
                if checked.elapsed() < LIVENESS_INTERVAL {
                    return CloudProcessRun::Running;
                }
                *checked = Instant::now();
                if group_alive(*group) {
                    return CloudProcessRun::Running;
                }
                CloudProcessRun::Ended(journal.exit_code())
            }
        }
    }
}

fn make_fifo(journal: &Journal) -> Result<File, &'static str> {
    let path = journal.input_path();
    if path.exists() {
        fs::remove_file(&path).map_err(|_| SPAWN_FAILED)?;
    }
    let made = Command::new(FIFO_PROGRAM)
        .args([FIFO_MODE_FLAG, FIFO_MODE, END_OF_OPTIONS])
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !made {
        tracing::debug!("could not make the input of a cloud process");
        return Err(SPAWN_FAILED);
    }
    open_input(journal).ok_or(SPAWN_FAILED)
}

fn open_input(journal: &Journal) -> Option<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(journal.input_path())
        .ok()
}

pub(super) fn start_piped(
    journal: Journal,
    launch: &CloudLaunch,
) -> Result<Arc<Tracked>, &'static str> {
    let (output, errors) = journal.create().map_err(|_| SPAWN_FAILED)?;
    let input = make_fifo(&journal)?;
    let child = Command::new(SHELL)
        .args([SHELL_COMMAND_FLAG, WRAPPER, WRAPPER_NAME])
        .arg(journal.input_path())
        .arg(journal.exit_path())
        .arg(journal.exit_temp_path())
        .arg(&launch.program)
        .args(&launch.args)
        .envs(launch.env.iter().map(|(name, value)| (name, value)))
        .current_dir(&launch.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(errors))
        .process_group(OWN_PROCESS_GROUP)
        .spawn()
        .map_err(|error| {
            tracing::debug!(%error, "could not start a cloud process");
            SPAWN_FAILED
        })?;
    let group = child.id();
    let tracked = Arc::new(Tracked::live(journal, Box::new(input), None, Some(group)));
    follow(&tracked, Watch::Child(child))?;
    Ok(tracked)
}

pub(super) fn adopt_piped(journal: Journal, group: u32) -> Option<Arc<Tracked>> {
    let input = open_input(&journal)?;
    let tracked = Arc::new(Tracked::adopted(journal, Box::new(input), group));
    let watch = Watch::Adopted {
        group,
        checked: Instant::now(),
    };
    follow(&tracked, watch).ok()?;
    Some(tracked)
}

fn follow(tracked: &Arc<Tracked>, mut watch: Watch) -> Result<(), &'static str> {
    let followed = Arc::clone(tracked);
    thread::Builder::new()
        .name(FOLLOW_THREAD.to_owned())
        .spawn(move || loop {
            followed.observe_length();
            if let CloudProcessRun::Ended(code) = watch.run(&followed.journal) {
                followed.observe_length();
                followed.record_exit(code);
                return;
            }
            thread::sleep(FOLLOW_INTERVAL);
        })
        .map(drop)
        .map_err(|_| SPAWN_FAILED)
}
