mod channels;
mod files;
mod login_env;

use krad_domain::{ExtensionMachine, InstallId};
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use crate::command::HostCommand;
use crate::error::{HostError, Waited};
use crate::failure::SshFailure;
use crate::layout::HostLayout;
use crate::local::hide_console_window;
use crate::machine::{MachineChannel, MachineReach};
use crate::mux::{MuxCarrier, MuxTools};
use crate::platform::{HostPlatform, UNAME_ARGS, UNAME_PROGRAM};
use crate::run::output_within;
use crate::ssh::{Interaction, SshArgs, Transport};
use channels::Carrier;

pub struct RemoteLink {
    pub(crate) args: SshArgs,
    install_id: Option<InstallId>,
    interactive: Mutex<Option<Carrier>>,
    bulk: Mutex<Option<Carrier>>,
    connection: Mutex<Option<HostPlatform>>,
    open: Arc<AtomicBool>,
    home: OnceLock<PathBuf>,
    platform: OnceLock<HostPlatform>,
    login_env: Mutex<Option<PathBuf>>,
    tools: MuxTools,
    mux: Mutex<Option<Arc<MuxCarrier>>>,
    machine: Option<MachineReach>,
}

impl fmt::Debug for RemoteLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteLink")
            .field("destination", &self.args.destination.to_string())
            .finish_non_exhaustive()
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl RemoteLink {
    #[must_use]
    pub fn new(args: SshArgs) -> Self {
        Self::with_install_id(args, None)
    }

    #[must_use]
    pub fn with_install_id(args: SshArgs, install_id: Option<InstallId>) -> Self {
        Self {
            args,
            install_id,
            interactive: Mutex::new(None),
            bulk: Mutex::new(None),
            connection: Mutex::new(None),
            open: Arc::new(AtomicBool::new(false)),
            home: OnceLock::new(),
            platform: OnceLock::new(),
            login_env: Mutex::new(None),
            tools: MuxTools::default(),
            mux: Mutex::new(None),
            machine: None,
        }
    }

    #[must_use]
    pub(crate) fn for_machine(
        args: SshArgs,
        install_id: Option<InstallId>,
        machine: ExtensionMachine,
    ) -> Self {
        Self {
            machine: Some(MachineReach::new(machine)),
            ..Self::with_install_id(args, install_id)
        }
    }

    #[must_use]
    pub fn machine(&self) -> Option<&ExtensionMachine> {
        self.machine.as_ref().map(|reach| &reach.machine)
    }

    pub fn set_machine_channel(&self, channel: MachineChannel) {
        if let Some(reach) = &self.machine {
            reach.set_channel(channel);
        }
    }

    pub(crate) fn refuse_ssh(&self) -> Result<(), HostError> {
        if self.machine.is_some() {
            return Err(HostError::NotConnected);
        }
        Ok(())
    }

    #[must_use]
    pub(crate) fn with_tools(mut self, tools: MuxTools) -> Self {
        self.tools = tools;
        self
    }

    pub(crate) fn multiplexed(&self) -> Option<Arc<MuxCarrier>> {
        lock(&self.mux).clone()
    }

    #[must_use]
    pub fn is_multiplexed(&self) -> bool {
        lock(&self.mux).is_some()
    }

    #[must_use]
    pub fn home(&self) -> Option<&Path> {
        self.home.get().map(PathBuf::as_path)
    }

    #[must_use]
    pub fn platform(&self) -> Option<HostPlatform> {
        self.platform.get().copied()
    }

    #[must_use]
    pub fn layout(&self) -> Option<HostLayout> {
        self.home()
            .map(|home| HostLayout::new(home, self.install_id.clone()))
    }

    pub(crate) fn liveness(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.open)
    }

    #[must_use]
    pub fn sign_in_terminal(&self) -> (OsString, Vec<OsString>) {
        (
            SshArgs::program(),
            self.args.sign_in_args(Interaction::Terminal),
        )
    }

    /// # Errors
    #[tracing::instrument(skip_all, fields(destination = %self.args.destination))]
    pub fn connect(&self) -> Result<HostPlatform, HostError> {
        let mut connection = lock(&self.connection);
        if let (Some(platform), true) = (*connection, self.is_open()) {
            return Ok(platform);
        }
        let platform = match self.args.transport {
            _ if self.machine.is_some() => self.start_multiplexed()?,
            Transport::ControlMaster => {
                self.args.ensure_control_dir()?;
                if !self.signed_in() {
                    self.sign_in()?;
                }
                self.detect_platform()?
            }
            Transport::Multiplexed => self.start_multiplexed()?,
        };
        self.platform.get_or_init(|| platform);
        let carrier = self.open_carrier()?;
        let home = carrier.sftp.realpath(channels::HOME_DIR)?;
        self.home.get_or_init(|| PathBuf::from(home));
        *lock(&self.interactive) = Some(carrier);
        self.open.store(true, Ordering::Release);
        if let Err(error) = self.refresh_environment() {
            tracing::warn!(%error, "could not capture the host's login environment");
        }
        *connection = Some(platform);
        tracing::info!(%platform, "connected to the host");
        Ok(platform)
    }

    fn start_multiplexed(&self) -> Result<HostPlatform, HostError> {
        if let Some(previous) = lock(&self.mux).take() {
            previous.close();
        }
        let (carrier, platform) = match &self.machine {
            Some(reach) => {
                let channel = reach.channel().ok_or(HostError::NoMachineChannel)?;
                MuxCarrier::through(&channel, &self.args, &self.tools)?
            }
            None => MuxCarrier::connect(&self.args, &self.tools)?,
        };
        *lock(&self.mux) = Some(Arc::new(carrier));
        Ok(platform)
    }

    fn signed_in(&self) -> bool {
        self.args.check_args().is_some_and(|args| {
            let mut check = ssh_command(&args);
            check.stdin(Stdio::null());
            output_within(
                check,
                self.args.timeouts.connect.duration(),
                Waited::Connect,
            )
            .is_ok_and(|out| out.status.success())
        })
    }

    fn sign_in(&self) -> Result<(), HostError> {
        let mut command = ssh_command(&self.args.sign_in_args(Interaction::Batch));
        command.stdin(Stdio::null());
        let output = output_within(
            command,
            self.args.timeouts.connect.duration(),
            Waited::Connect,
        )?;
        if output.status.success() {
            return Ok(());
        }
        let failure = SshFailure::classify(&String::from_utf8_lossy(&output.stderr));
        tracing::debug!(%failure, "batch sign-in failed");
        Err(HostError::Ssh(failure))
    }

    pub(crate) fn run_plain(
        &self,
        command: &HostCommand,
        waited: Waited,
    ) -> Result<Output, HostError> {
        let mut built = if let Some(mux) = self.multiplexed() {
            mux.command(command.plain_script())
        } else {
            self.refuse_ssh()?;
            command.over_ssh_plain(&self.args, |script| self.stage_script(script))?
        };
        built.stdin(Stdio::null());
        Self::checked(output_within(
            built,
            self.args.timeouts.command.duration(),
            waited,
        )?)
    }

    pub(crate) fn checked(output: Output) -> Result<Output, HostError> {
        if SshFailure::is_ssh_status(output.status.code()) {
            let failure = SshFailure::classify(&String::from_utf8_lossy(&output.stderr));
            if !matches!(failure, SshFailure::Other(_)) {
                return Err(HostError::Ssh(failure));
            }
        }
        Ok(output)
    }

    fn detect_platform(&self) -> Result<HostPlatform, HostError> {
        let mut uname = HostCommand::new(UNAME_PROGRAM);
        uname.args(UNAME_ARGS);
        let output = self.run_plain(&uname, Waited::Connect)?;
        if !output.status.success() {
            return Err(HostError::Remote {
                program: UNAME_PROGRAM.to_owned(),
                status: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).parse()?)
    }

    pub fn close(&self) {
        self.open.store(false, Ordering::Release);
        lock(&self.interactive).take();
        lock(&self.bulk).take();
        lock(&self.connection).take();
        if let Some(mux) = lock(&self.mux).take() {
            mux.close();
        }
        if let Some(args) = self.args.sign_out_args().filter(|_| self.machine.is_none()) {
            let mut exit = ssh_command(&args);
            exit.stdin(Stdio::null());
            output_within(exit, self.args.timeouts.connect.duration(), Waited::Connect).ok();
        }
        tracing::info!(destination = %self.args.destination, "closed the host connection");
    }
}

pub(crate) fn ssh_command(args: &[OsString]) -> Command {
    let mut command = Command::new(SshArgs::program());
    command.args(args);
    hide_console_window(&mut command);
    command
}
