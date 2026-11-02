use krad_domain::Host;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::Arc;

use crate::command::HostCommand;
use crate::context::HostContext;
use crate::error::{HostError, Waited};
pub use crate::metadata::Metadata;
use crate::mux::MuxTools;
use crate::path::remote_path;
use crate::removal::is_removable;
use crate::run::output_within;
use crate::ssh::SshArgs;
pub use crate::ssh_link::SshLink;

const BULK_THRESHOLD_BYTES: usize = 1024 * 1024;
const REMOVE_PROGRAM: &str = "rm";
const REMOVE_TREE_ARGS: [&str; 2] = ["-rf", "--"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub metadata: Metadata,
    pub symlink: bool,
}

#[derive(Clone, Debug, Default)]
pub enum HostLink {
    #[default]
    Local,
    Ssh(Arc<SshLink>),
}

impl PartialEq for HostLink {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Local, Self::Local) => true,
            (Self::Ssh(left), Self::Ssh(right)) => {
                Arc::ptr_eq(left, right) || left.args == right.args
            }
            _ => false,
        }
    }
}

impl Eq for HostLink {}

impl HostLink {
    #[must_use]
    pub fn new(host: &Host, context: &HostContext) -> Self {
        match host {
            Host::Local => Self::Local,
            Host::Ssh { destination } => {
                let args = SshArgs::new(destination.clone(), context.config_dir.as_deref())
                    .with_timeouts(context.timeouts)
                    .with_transport(context.transport.unwrap_or_default());
                Self::Ssh(Arc::new(
                    SshLink::with_install_id(args, context.install_id.clone()).with_tools(
                        MuxTools {
                            local_cli: context.local_cli.clone(),
                            host_clis: context.host_clis.clone(),
                            prompts: context.prompts.clone(),
                            config_dir: context.config_dir.clone(),
                        },
                    ),
                ))
            }
        }
    }

    #[must_use]
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        match self {
            Self::Local => true,
            Self::Ssh(ssh) => ssh.is_open(),
        }
    }

    #[must_use]
    pub fn host(&self) -> Host {
        match self {
            Self::Local => Host::Local,
            Self::Ssh(ssh) => Host::Ssh {
                destination: ssh.args.destination.clone(),
            },
        }
    }

    /// # Errors
    pub fn metadata(&self, path: &Path) -> Result<Metadata, HostError> {
        match self {
            Self::Local => Ok(fs::metadata(path)?.into()),
            Self::Ssh(ssh) => ssh.with_sftp(|sftp| sftp.stat(&remote_path(path))),
        }
    }

    /// # Errors
    pub fn symlink_metadata(&self, path: &Path) -> Result<Metadata, HostError> {
        match self {
            Self::Local => Ok(fs::symlink_metadata(path)?.into()),
            Self::Ssh(ssh) => ssh.with_sftp(|sftp| sftp.lstat(&remote_path(path))),
        }
    }

    /// # Errors
    pub fn create_new(&self, path: &Path) -> Result<(), HostError> {
        match self {
            Self::Local => {
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)?;
                Ok(())
            }
            Self::Ssh(ssh) => ssh.with_sftp(|sftp| sftp.create_new(&remote_path(path))),
        }
    }

    #[must_use]
    pub fn exists(&self, path: &Path) -> bool {
        self.metadata(path).is_ok()
    }

    /// # Errors
    pub fn read_dir(&self, dir: &Path) -> Result<Vec<Entry>, HostError> {
        match self {
            Self::Local => {
                let mut entries = Vec::new();
                for entry in fs::read_dir(dir)? {
                    let entry = entry?;
                    let symlink = entry.file_type()?.is_symlink();
                    let path = entry.path();
                    let metadata = fs::metadata(&path).or_else(|_| fs::symlink_metadata(&path))?;
                    entries.push(Entry {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        path,
                        metadata: metadata.into(),
                        symlink,
                    });
                }
                Ok(entries)
            }
            Self::Ssh(ssh) => {
                let entries = ssh.with_sftp(|sftp| sftp.read_dir(&remote_path(dir)))?;
                Ok(entries
                    .into_iter()
                    .map(|entry| Entry {
                        path: dir.join(&entry.name),
                        name: entry.name,
                        metadata: entry.metadata,
                        symlink: entry.symlink,
                    })
                    .collect())
            }
        }
    }

    /// # Errors
    pub fn read(&self, path: &Path, limit: u64) -> Result<Vec<u8>, HostError> {
        match self {
            Self::Local => {
                let mut bytes = Vec::new();
                fs::File::open(path)?.take(limit).read_to_end(&mut bytes)?;
                Ok(bytes)
            }
            Self::Ssh(ssh) => ssh.with_sftp(|sftp| sftp.read(&remote_path(path), limit)),
        }
    }

    /// # Errors
    pub fn read_range(&self, path: &Path, from: u64, limit: u64) -> Result<Vec<u8>, HostError> {
        match self {
            Self::Local => {
                let mut file = fs::File::open(path)?;
                file.seek(SeekFrom::Start(from))?;
                let mut bytes = Vec::new();
                file.take(limit).read_to_end(&mut bytes)?;
                Ok(bytes)
            }
            Self::Ssh(ssh) => {
                ssh.with_sftp(|sftp| sftp.read_range(&remote_path(path), from, limit))
            }
        }
    }

    /// # Errors
    pub fn read_all(&self, path: &Path) -> Result<Vec<u8>, HostError> {
        self.read(path, u64::MAX)
    }

    /// # Errors
    pub fn read_to_string(&self, path: &Path) -> Result<String, HostError> {
        let bytes = self.read_all(path)?;
        String::from_utf8(bytes).map_err(|error| {
            HostError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })
    }

    /// # Errors
    pub fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), HostError> {
        match self {
            Self::Local => Ok(fs::write(path, bytes)?),
            Self::Ssh(ssh) => {
                let target = remote_path(path);
                let save = |sftp: &crate::sftp::Sftp| sftp.save(&target, bytes).map(drop);
                if bytes.len() > BULK_THRESHOLD_BYTES {
                    ssh.with_bulk(save)
                } else {
                    ssh.with_sftp(save)
                }
            }
        }
    }

    /// # Errors
    pub fn create_dir(&self, path: &Path) -> Result<(), HostError> {
        match self {
            Self::Local => Ok(fs::create_dir(path)?),
            Self::Ssh(ssh) => ssh.with_sftp(|sftp| sftp.mkdir(&remote_path(path))),
        }
    }

    /// # Errors
    pub fn create_dir_all(&self, path: &Path) -> Result<(), HostError> {
        match self {
            Self::Local => Ok(fs::create_dir_all(path)?),
            Self::Ssh(_) => {
                if self.metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
                    return Ok(());
                }
                if let Some(parent) = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                {
                    self.create_dir_all(parent)?;
                }
                self.create_dir(path)
            }
        }
    }

    /// # Errors
    pub fn remove_file(&self, path: &Path) -> Result<(), HostError> {
        match self {
            Self::Local => Ok(fs::remove_file(path)?),
            Self::Ssh(ssh) => ssh.with_sftp(|sftp| sftp.remove_file(&remote_path(path))),
        }
    }

    /// # Errors
    pub fn remove_dir_all(&self, path: &Path) -> Result<(), HostError> {
        match self {
            Self::Local => Ok(fs::remove_dir_all(path)?),
            Self::Ssh(_) => self.remove_tree_on_host(path),
        }
    }

    /// # Errors
    pub fn rename(&self, from: &Path, to: &Path) -> Result<(), HostError> {
        match self {
            Self::Local => Ok(fs::rename(from, to)?),
            Self::Ssh(ssh) => {
                ssh.with_sftp(|sftp| sftp.rename(&remote_path(from), &remote_path(to)))
            }
        }
    }

    /// # Errors
    pub fn canonicalize(&self, path: &Path) -> Result<PathBuf, HostError> {
        match self {
            Self::Local => Ok(fs::canonicalize(path)?),
            Self::Ssh(ssh) => ssh
                .with_sftp(|sftp| sftp.realpath(&remote_path(path)))
                .map(PathBuf::from),
        }
    }

    /// # Errors
    #[tracing::instrument(skip_all, fields(program = %command.program_name()))]
    pub fn output(&self, command: &HostCommand) -> Result<Output, HostError> {
        let mut built = command.build(self)?;
        built.stdin(Stdio::null());
        match self {
            Self::Local => Ok(built.output()?),
            Self::Ssh(ssh) => SshLink::checked(output_within(
                built,
                ssh.args.timeouts.command.duration(),
                Waited::Command,
            )?),
        }
    }

    /// # Errors
    #[tracing::instrument(skip_all, fields(program = %command.program_name()))]
    pub fn output_within(
        &self,
        command: &HostCommand,
        timeout: std::time::Duration,
    ) -> Result<Output, HostError> {
        let mut built = command.build(self)?;
        built.stdin(Stdio::null());
        let output = output_within(built, timeout, Waited::Command)?;
        match self {
            Self::Local => Ok(output),
            Self::Ssh(_) => SshLink::checked(output),
        }
    }

    fn remove_tree_on_host(&self, path: &Path) -> Result<(), HostError> {
        let unsafe_removal = || HostError::UnsafeRemoval(remote_path(path));
        let Self::Ssh(ssh) = self else {
            return Err(unsafe_removal());
        };
        if !is_removable(path, ssh.home()) {
            return Err(unsafe_removal());
        }
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(unsafe_removal());
        };
        let resolved = self.canonicalize(parent)?.join(name);
        if !is_removable(&resolved, ssh.home()) {
            return Err(unsafe_removal());
        }
        let path = resolved.as_path();
        let mut remove = HostCommand::new(REMOVE_PROGRAM);
        remove.args(REMOVE_TREE_ARGS).arg(remote_path(path));
        let output = self.output(&remove)?;
        if output.status.success() {
            return Ok(());
        }
        Err(HostError::Remote {
            program: REMOVE_PROGRAM.to_owned(),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}
