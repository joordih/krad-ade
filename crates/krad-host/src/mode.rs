use std::path::{Path, PathBuf};

use crate::error::HostError;
use crate::link::HostLink;
use crate::local;
use crate::path::remote_path;
use crate::ssh_link::SshLink;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileMode(pub u32);

impl FileMode {
    pub const PRIVATE_FILE: Self = Self(0o600);
    pub const PRIVATE_DIR: Self = Self(0o700);
    pub const EXECUTABLE: Self = Self(0o755);
}

impl SshLink {
    pub(crate) fn write_with_mode(
        &self,
        path: &Path,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), HostError> {
        let target = remote_path(path);
        self.with_sftp(|sftp| {
            if sftp.lstat(&target).is_ok() {
                sftp.set_permissions(&target, mode.0)?;
            }
            sftp.write_with_mode(&target, bytes, Some(mode.0))?;
            sftp.set_permissions(&target, mode.0)
        })
    }

    pub(crate) fn create_dir_with_mode(
        &self,
        path: &Path,
        mode: FileMode,
    ) -> Result<(), HostError> {
        let mut missing: Vec<PathBuf> = Vec::new();
        let mut at = Some(path);
        while let Some(dir) = at.filter(|dir| !dir.as_os_str().is_empty()) {
            let found = self.with_sftp(|sftp| sftp.stat(&remote_path(dir)));
            match found {
                Ok(metadata) if metadata.is_dir() => break,
                Ok(_) => {
                    return Err(HostError::Io(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "a file stands where a folder is needed",
                    )))
                }
                Err(error) if error.is_not_found() => missing.push(dir.to_owned()),
                Err(error) => return Err(error),
            }
            at = dir.parent();
        }
        for dir in missing.iter().rev() {
            let created =
                self.with_sftp(|sftp| sftp.mkdir_with_mode(&remote_path(dir), Some(mode.0)));
            if let Err(error) = created {
                if !self
                    .with_sftp(|sftp| sftp.stat(&remote_path(dir)))
                    .is_ok_and(|metadata| metadata.is_dir())
                {
                    return Err(error);
                }
            }
        }
        let target = remote_path(path);
        self.with_sftp(|sftp| sftp.set_permissions(&target, mode.0))
    }
}

impl HostLink {
    /// # Errors
    pub fn write_with_mode(
        &self,
        path: &Path,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), HostError> {
        match self {
            Self::Local => Ok(local::write_with_mode(path, bytes, mode.0)?),
            Self::Ssh(ssh) => ssh.write_with_mode(path, bytes, mode),
        }
    }

    /// # Errors
    pub fn create_dir_with_mode(&self, path: &Path, mode: FileMode) -> Result<(), HostError> {
        match self {
            Self::Local => {
                if let Some(parent) = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                {
                    std::fs::create_dir_all(parent)?;
                }
                if !path.is_dir() {
                    local::create_dir_with_mode(path, mode.0)?;
                }
                Ok(local::set_permissions(path, mode.0)?)
            }
            Self::Ssh(ssh) => ssh.create_dir_with_mode(path, mode),
        }
    }

    /// # Errors
    pub fn set_permissions(&self, path: &Path, mode: FileMode) -> Result<(), HostError> {
        match self {
            Self::Local => Ok(local::set_permissions(path, mode.0)?),
            Self::Ssh(ssh) => {
                ssh.with_sftp(|sftp| sftp.set_permissions(&remote_path(path), mode.0))
            }
        }
    }
}
