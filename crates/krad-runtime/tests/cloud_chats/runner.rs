use krad_host::runner::{run_client_verb, runner_root, runner_socket, runtime_socket, RUNNER_VERB};
use krad_host::{HostLayout, APP_DIR, HOST_CLI_BUILD, HOST_CLI_PROGRAM};
use krad_runner::{run_runner_verb, serve_runner};
use krad_store::Secrets;
use std::error::Error;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const CLI_MODE_VAR: &str = "KRAD_TEST_HOST_CLI";
const CLI_MODE_ON: &str = "1";
const HOME_VAR: &str = "HOME";
const VERSION_FLAG: &str = "--version";
const EXECUTABLE: u32 = 0o755;
const BIN_DIR: &str = "bin";
const EXIT_FAILED: u8 = 1;
const SOCKET_WAIT: Duration = Duration::from_secs(10);
const SOCKET_POLL: Duration = Duration::from_millis(20);
const PROCESSES_DIR: &str = "processes";
const PID_FILE: &str = "pid";
const KILL_PROGRAM: &str = "kill";
const KILL_SIGNAL: &str = "-KILL";
const NO_SOCKET: &str = "the cloud runner daemon never opened its socket";
const PATH_VAR: &str = "PATH";
const SHELL_VAR: &str = "SHELL";
const SHELL: &str = "/bin/sh";
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const FAKE_SSH_DIR: &str = "tests/fixtures/fake-ssh";
const RUNNER_BIN: &str = "runner-bin";
const HOST_BIN: &str = "host-bin";

pub fn serve_as_host_cli() -> Option<ExitCode> {
    std::env::var_os(CLI_MODE_VAR)?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((verb, rest)) = args.split_first() else {
        return Some(ExitCode::SUCCESS);
    };
    if verb == VERSION_FLAG {
        drop(writeln!(std::io::stdout(), "{HOST_CLI_BUILD}"));
        return Some(ExitCode::SUCCESS);
    }
    let code = if verb == RUNNER_VERB {
        runner_root()
            .and_then(|root| serve_runner(&root, Secrets::in_memory()))
            .unwrap_or(i32::from(EXIT_FAILED))
    } else {
        run_runner_verb(verb, rest)
            .or_else(|| run_client_verb(verb, rest))
            .unwrap_or_default()
    };
    Some(ExitCode::from(u8::try_from(code).unwrap_or(EXIT_FAILED)))
}

pub fn install_host_cli(runner_home: &Path) -> Result<(), Box<dyn Error>> {
    let bin = runner_home.join(APP_DIR).join(HOST_CLI_BUILD).join(BIN_DIR);
    std::fs::create_dir_all(&bin)?;
    let cli = bin.join(HOST_CLI_PROGRAM);
    let exe = std::env::current_exe()?;
    let script = format!(
        "#!/bin/sh\n{CLI_MODE_VAR}={CLI_MODE_ON}\nexport {CLI_MODE_VAR}\nexec '{}' \"$@\"\n",
        exe.display()
    );
    std::fs::write(&cli, script)?;
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(EXECUTABLE))?;
    Ok(())
}

pub fn runner_dir(runner_home: &Path) -> PathBuf {
    HostLayout::new(runner_home, None).runner_dir()
}

pub struct Daemon {
    child: Child,
    home: PathBuf,
    processes: PathBuf,
}

impl Daemon {
    fn stop_processes(&self) {
        let groups = std::fs::read_dir(&self.processes)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| std::fs::read_to_string(entry.path().join(PID_FILE)).ok());
        for group in groups {
            drop(
                Command::new(KILL_PROGRAM)
                    .arg(KILL_SIGNAL)
                    .arg(format!("-{}", group.trim()))
                    .stderr(Stdio::null())
                    .status(),
            );
        }
    }

    pub fn restart(&mut self) -> Result<(), Box<dyn Error>> {
        self.child.kill()?;
        self.child.wait()?;
        self.child = spawn_daemon(&self.home)?;
        wait_for_runtime(&self.home)
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
        self.stop_processes();
    }
}

fn runner_path() -> String {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join(FAKE_SSH_DIR);
    format!(
        "{}:{}:{SYSTEM_PATH}",
        fixtures.join(RUNNER_BIN).display(),
        fixtures.join(HOST_BIN).display()
    )
}

fn spawn_daemon(runner_home: &Path) -> Result<Child, Box<dyn Error>> {
    Ok(Command::new(std::env::current_exe()?)
        .arg(RUNNER_VERB)
        .env(CLI_MODE_VAR, CLI_MODE_ON)
        .env(HOME_VAR, runner_home)
        .env(PATH_VAR, runner_path())
        .env(SHELL_VAR, SHELL)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?)
}

fn wait_for_runtime(runner_home: &Path) -> Result<(), Box<dyn Error>> {
    let socket = runtime_socket(&runner_dir(runner_home));
    let deadline = Instant::now() + SOCKET_WAIT;
    while UnixStream::connect(&socket).is_err() {
        if Instant::now() > deadline {
            return Err(NO_SOCKET.into());
        }
        std::thread::sleep(SOCKET_POLL);
    }
    Ok(())
}

pub fn start_daemon(runner_home: &Path) -> Result<Daemon, Box<dyn Error>> {
    let socket = runner_socket(&runner_dir(runner_home));
    let daemon = Daemon {
        child: spawn_daemon(runner_home)?,
        home: runner_home.to_owned(),
        processes: runner_dir(runner_home).join(PROCESSES_DIR),
    };
    let deadline = Instant::now() + SOCKET_WAIT;
    while !socket.exists() {
        if Instant::now() > deadline {
            return Err(NO_SOCKET.into());
        }
        std::thread::sleep(SOCKET_POLL);
    }
    Ok(daemon)
}
