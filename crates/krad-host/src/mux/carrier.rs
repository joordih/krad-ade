use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufReader, Read};
use std::net::{Ipv4Addr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::askpass::askpass_alias;
use super::bootstrap::{self, BootstrapFailure, Greeting};
use super::bridge::splice_tcp;
use super::broker::Broker;
use super::channel::Channel;
use super::prompt::PromptSender;
use super::session::{Closer, Incoming, Session, Side};
use super::wire::OpenRequest;
use super::{MuxError, EXEC_VERB};
use crate::command::transport;
use crate::error::{HostError, Waited};
use crate::failure::SshFailure;
use crate::layout::APP_VERSION;
use crate::local::hide_console_window;
use crate::platform::{HostPlatform, UnsupportedPlatform};
use crate::ssh::SshArgs;
use crate::ssh_link::lock;

const ASKPASS_VAR: &str = "SSH_ASKPASS";
const ASKPASS_REQUIRE_VAR: &str = "SSH_ASKPASS_REQUIRE";
const ASKPASS_FORCED: &str = "force";
const DISPLAY_VAR: &str = "DISPLAY";
const PLACEHOLDER_DISPLAY: &str = "krad-ade:0";
const SIGN_IN_WAIT: Duration = Duration::from_mins(5);
const STDERR_LIMIT_BYTES: u64 = 16 * 1024;
const BOOT_THREAD: &str = "krad-ade-mux-boot";
const ERRORS_THREAD: &str = "krad-ade-mux-ssh-errors";
const ACCEPT_THREAD: &str = "krad-ade-mux-accepted";
const STDERR_SETTLE: Duration = Duration::from_millis(100);
const BOOT_EXIT_WAIT: Duration = Duration::from_secs(2);
const EXIT_POLL: Duration = Duration::from_millis(20);
const HOST_KEY_NOT_TRUSTED: &str = "the host's key was not trusted";
const UNKNOWN_LISTENER: &str = "no forward waits for that listener";
const LOCAL_TARGET_DOWN: &str = "nothing listens on that port of this computer";

#[derive(Clone, Debug, Default)]
pub(crate) struct MuxTools {
    pub(crate) local_cli: Option<PathBuf>,
    pub(crate) host_clis: Vec<(HostPlatform, PathBuf)>,
    pub(crate) prompts: Option<PromptSender>,
    pub(crate) config_dir: Option<PathBuf>,
}

pub(crate) struct MuxCarrier {
    child: Arc<Mutex<Child>>,
    session: Session,
    broker: Broker,
    listeners: Arc<Mutex<HashMap<u32, u16>>>,
    local_cli: PathBuf,
}

type Booted = Result<(Session, mpsc::Receiver<Incoming>, Greeting), MuxError>;

impl MuxCarrier {
    #[tracing::instrument(skip_all, fields(destination = %args.destination))]
    pub(crate) fn connect(
        args: &SshArgs,
        tools: &MuxTools,
    ) -> Result<(Self, HostPlatform), HostError> {
        let local_cli = tools.local_cli.clone().ok_or(MuxError::NoLocalCli)?;
        let askpass = askpass_alias(&local_cli, tools.config_dir.as_deref())?;
        let broker = Broker::start(args.destination.clone(), tools.prompts.clone())?;
        let mut command = Command::new(SshArgs::program());
        command
            .args(args.multiplexed_args(OsString::from(
                transport::inline(&bootstrap::script()).ok_or(MuxError::Protocol)?,
            )))
            .env(ASKPASS_VAR, &askpass)
            .env(ASKPASS_REQUIRE_VAR, ASKPASS_FORCED)
            .envs(broker.env())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if std::env::var_os(DISPLAY_VAR).is_none() {
            command.env(DISPLAY_VAR, PLACEHOLDER_DISPLAY);
        }
        hide_console_window(&mut command);
        let mut child = command.spawn()?;
        let errors = collect_errors(&mut child)?;
        let pipes = child.stdin.take().zip(child.stdout.take());
        let child = Arc::new(Mutex::new(child));
        let Some((stdin, stdout)) = pipes else {
            stop(&child);
            return Err(HostError::NotConnected);
        };
        let booted = boot(stdin, stdout, tools.host_clis.clone(), &child, args);
        let (session, incoming, greeting) = match booted {
            Ok(booted) => booted,
            Err(error) => {
                let failure = exited_within(&child, BOOT_EXIT_WAIT)
                    .and_then(BootstrapFailure::from_exit_code);
                stop(&child);
                if let Some(failure) = failure {
                    return Err(MuxError::Bootstrap(failure).into());
                }
                return Err(boot_failure(error, &errors));
            }
        };
        broker.attach(session.clone());
        let listeners = Arc::new(Mutex::new(HashMap::new()));
        accept_forwarded(incoming, Arc::clone(&listeners))?;
        tracing::info!(platform = %greeting.platform, "multiplexed session open");
        Ok((
            Self {
                child,
                session,
                broker,
                listeners,
                local_cli,
            },
            greeting.platform,
        ))
    }

    pub(crate) fn is_alive(&self) -> bool {
        !self.session.is_closed()
            && lock(&self.child)
                .try_wait()
                .is_ok_and(|status| status.is_none())
    }

    pub(crate) fn open(
        &self,
        request: OpenRequest,
        wait: Duration,
    ) -> Result<(Channel, Option<u16>), HostError> {
        Ok(self.session.open(request, wait)?)
    }

    pub(crate) fn command(&self, script: String) -> Command {
        let id = self.broker.register(script);
        let mut command = Command::new(&self.local_cli);
        command.arg(EXEC_VERB).arg(id).envs(self.broker.env());
        hide_console_window(&mut command);
        command
    }

    pub(crate) fn listen_for(&self, channel: u32, local_port: u16) {
        lock(&self.listeners).insert(channel, local_port);
    }

    pub(crate) fn forget_listener(&self, channel: u32) {
        lock(&self.listeners).remove(&channel);
    }

    pub(crate) fn close(&self) {
        self.session.close();
        stop(&self.child);
    }
}

impl Drop for MuxCarrier {
    fn drop(&mut self) {
        self.close();
    }
}

fn boot(
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    host_clis: Vec<(HostPlatform, PathBuf)>,
    child: &Arc<Mutex<Child>>,
    args: &SshArgs,
) -> Booted {
    let (sender, booted) = mpsc::channel();
    let killer = Arc::clone(child);
    let handshake = args.timeouts.connect.duration();
    thread::Builder::new()
        .name(BOOT_THREAD.to_owned())
        .spawn(move || {
            let result = (|| {
                let mut reader = BufReader::new(stdout);
                let greeting = bootstrap::read_greeting(&mut reader)?;
                bootstrap::answer(&mut stdin, &greeting, &host_clis)?;
                let closer: Closer = Box::new(move || stop(&killer));
                let (session, incoming, _) = Session::start(
                    reader,
                    stdin,
                    Side::Client,
                    APP_VERSION,
                    handshake,
                    Some(closer),
                )?;
                Ok((session, incoming, greeting))
            })();
            sender.send(result).ok();
        })
        .map_err(MuxError::Io)?;
    booted
        .recv_timeout(SIGN_IN_WAIT)
        .map_err(|_| MuxError::TimedOut)?
}

fn exited_within(child: &Mutex<Child>, wait: Duration) -> Option<i32> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        if let Ok(Some(status)) = lock(child).try_wait() {
            return status.code();
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        thread::sleep(EXIT_POLL);
    }
}

fn stop(child: &Mutex<Child>) {
    let mut child = lock(child);
    if child.kill().is_ok() {
        child.wait().ok();
    }
}

fn collect_errors(child: &mut Child) -> Result<Arc<Mutex<Vec<u8>>>, HostError> {
    let collected = Arc::new(Mutex::new(Vec::new()));
    if let Some(stderr) = child.stderr.take() {
        let sink = Arc::clone(&collected);
        thread::Builder::new()
            .name(ERRORS_THREAD.to_owned())
            .spawn(move || {
                let mut bytes = Vec::new();
                let mut limited = stderr.take(STDERR_LIMIT_BYTES);
                limited.read_to_end(&mut bytes).ok();
                *lock(&sink) = bytes;
                std::io::copy(&mut limited.into_inner(), &mut std::io::sink()).ok();
            })?;
    }
    Ok(collected)
}

fn boot_failure(error: MuxError, errors: &Mutex<Vec<u8>>) -> HostError {
    match error {
        MuxError::Closed | MuxError::Io(_) | MuxError::Protocol => {
            thread::sleep(STDERR_SETTLE);
            let stderr = String::from_utf8_lossy(&lock(errors)).into_owned();
            let failure = match SshFailure::classify(&stderr) {
                SshFailure::NeedsInteractiveSignIn => SshFailure::AuthenticationFailed,
                SshFailure::HostKeyUnknown => SshFailure::Other(HOST_KEY_NOT_TRUSTED.to_owned()),
                failure => failure,
            };
            tracing::debug!(%failure, "the multiplexed session did not start");
            HostError::Ssh(failure)
        }
        MuxError::TimedOut => HostError::TimedOut(Waited::Connect),
        MuxError::Unsupported(uname) => HostError::UnsupportedPlatform(UnsupportedPlatform(uname)),
        other => HostError::Mux(other),
    }
}

fn accept_forwarded(
    incoming: mpsc::Receiver<Incoming>,
    listeners: Arc<Mutex<HashMap<u32, u16>>>,
) -> Result<(), HostError> {
    thread::Builder::new()
        .name(ACCEPT_THREAD.to_owned())
        .spawn(move || {
            for Incoming { channel, request } in incoming {
                let OpenRequest::Accepted { listener } = request else {
                    channel.refuse(UNKNOWN_LISTENER);
                    continue;
                };
                let Some(port) = lock(&listeners).get(&listener).copied() else {
                    channel.refuse(UNKNOWN_LISTENER);
                    continue;
                };
                let spawned =
                    thread::Builder::new()
                        .name(ACCEPT_THREAD.to_owned())
                        .spawn(
                            move || match TcpStream::connect((Ipv4Addr::LOCALHOST, port)) {
                                Ok(tcp) => {
                                    if channel.accept(None).is_ok() {
                                        splice_tcp(&channel, &tcp).ok();
                                    }
                                }
                                Err(_) => channel.refuse(LOCAL_TARGET_DOWN),
                            },
                        );
                if let Err(error) = spawned {
                    tracing::warn!(%error, "could not take a forwarded connection");
                }
            }
        })?;
    Ok(())
}
