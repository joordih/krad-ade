use crate::error::AgentsError;
use crate::protocol::{ApiInfo, ApiVersion, BusToken, Timestamp, API_VERSION};
use crate::server::Endpoint;
use krad_host::{FileMode, HostLink};
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub const API_FILE_NAME: &str = "api.json";
const TMP_SUFFIX: &str = ".tmp";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiDiscovery {
    pub api_version: ApiVersion,
    pub endpoint: SocketAddr,
    pub token: BusToken,
    pub pid: u32,
    pub started_at: Timestamp,
}

impl ApiDiscovery {
    #[must_use]
    pub fn new(endpoint: &Endpoint, started_at: Timestamp) -> Self {
        Self {
            api_version: API_VERSION,
            endpoint: endpoint.address,
            token: endpoint.token.clone(),
            pid: std::process::id(),
            started_at,
        }
    }
}

#[must_use]
pub fn liveness(written_by: u32, answer: Option<&ApiInfo>) -> Liveness {
    match answer {
        None => Liveness::NotAnswering,
        Some(info) if info.pid.is_some_and(|pid| pid != written_by) => Liveness::OtherProcess,
        Some(_) => Liveness::Live,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Liveness {
    Live,
    NotAnswering,
    OtherProcess,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiFile {
    pub path: PathBuf,
}

impl ApiFile {
    #[must_use]
    pub fn in_config(dir: &Path) -> Self {
        Self {
            path: dir.join(API_FILE_NAME),
        }
    }

    fn tmp_path(&self) -> PathBuf {
        let mut tmp = self.path.as_os_str().to_owned();
        tmp.push(TMP_SUFFIX);
        PathBuf::from(tmp)
    }

    /// # Errors
    pub fn write(&self, discovery: &ApiDiscovery) -> Result<(), AgentsError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.tmp_path();
        remove_if_present(&tmp)?;
        let bytes = serde_json::to_vec_pretty(discovery)?;
        HostLink::Local.write_with_mode(&tmp, &bytes, FileMode::PRIVATE_FILE)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// # Errors
    pub fn read(&self) -> Result<Option<ApiDiscovery>, AgentsError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// # Errors
    pub fn remove(&self) -> Result<(), AgentsError> {
        Ok(remove_if_present(&self.path)?)
    }

    /// # Errors
    pub fn remove_if_written_by(&self, pid: u32) -> Result<(), AgentsError> {
        let written_by_us = match self.read() {
            Ok(found) => found.is_some_and(|found| found.pid == pid),
            Err(AgentsError::Json(_)) => false,
            Err(error) => return Err(error),
        };
        if written_by_us {
            remove_if_present(&self.path)?;
        }
        Ok(())
    }
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
