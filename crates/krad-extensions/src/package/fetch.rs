use krad_host::HostLink;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use ureq::Agent;

use super::on_host::{self, Download, HostFetch};
use super::{Installing, DESTINATION};
use crate::archive::{kind_of, place};
use crate::error::ExtensionError;
use crate::http::{download, release_by_tag};
use crate::platform::{Os, Platform};
use crate::registry::{Entry, Package};
use crate::server_runtime::FindServerRuntime;
use crate::template::Vars;

const FILE_FIELD: &str = "file";
const FILES_PREFIX: &str = "files.";
const GITHUB_DOWNLOADS: &str = "https://github.com";
const DOWNLOAD_SUFFIX: &str = ".download";
const STAGING_PREFIX: &str = "krad-language-";

static NEXT_STAGING: AtomicU64 = AtomicU64::new(0);

struct Staging(PathBuf);

impl Staging {
    fn new(into: &Path) -> Result<Self, ExtensionError> {
        let install = NEXT_STAGING.fetch_add(1, Ordering::Relaxed);
        let name = into
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let dir = std::env::temp_dir().join(format!(
            "{STAGING_PREFIX}{}-{install}-{name}",
            std::process::id()
        ));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            tracing::debug!(error = %error, "could not remove a staging folder");
        }
    }
}

impl<F: FindServerRuntime> Installing<'_, F> {
    pub(super) fn fetch_github(
        &self,
        package: &Package,
        entry: &Entry,
        vars: &Vars,
        dir: &Path,
    ) -> Result<(), ExtensionError> {
        let file = vars
            .get(&format!("{}.{FILE_FIELD}", Package::entry_prefix(entry)))
            .ok_or(ExtensionError::NoBuild)?;
        let (name, destination) = file.split_once(DESTINATION).unwrap_or((file, ""));
        let repository = &package.purl.name;
        let version = &package.purl.version;
        let digest = release_by_tag(self.agent, repository, version)
            .ok()
            .and_then(|release| {
                release
                    .assets
                    .into_iter()
                    .find(|asset| asset.name == name)
                    .and_then(|asset| asset.sha256())
            });
        let url = format!("{GITHUB_DOWNLOADS}/{repository}/releases/download/{version}/{name}");
        self.fetch_one(
            &Download {
                url: &url,
                name,
                kind: kind_of(name),
                digest: digest.as_ref(),
            },
            &dir.join(destination),
        )
    }

    pub(super) fn fetch_generic(
        &self,
        entry: &Entry,
        vars: &Vars,
        dir: &Path,
    ) -> Result<(), ExtensionError> {
        let prefix = format!("{}.{FILES_PREFIX}", Package::entry_prefix(entry));
        let files: Vec<(String, String)> = entry
            .fields
            .keys()
            .filter_map(|key| key.strip_prefix(FILES_PREFIX))
            .filter_map(|name| {
                let url = vars.get(&format!("{prefix}{name}"))?;
                Some((name.to_owned(), url.to_owned()))
            })
            .collect();
        if files.is_empty() {
            return Err(ExtensionError::NoBuild);
        }
        for (name, url) in files {
            self.fetch_one(
                &Download {
                    url: &url,
                    name: &name,
                    kind: kind_of(&name),
                    digest: None,
                },
                dir,
            )?;
        }
        Ok(())
    }

    fn fetch_one(&self, download: &Download<'_>, into: &Path) -> Result<(), ExtensionError> {
        fetch_to(self.link, self.agent, self.platform.os, download, into)
    }
}

pub(crate) fn fetch_to(
    link: &HostLink,
    agent: &Agent,
    target_os: Os,
    download: &Download<'_>,
    into: &Path,
) -> Result<(), ExtensionError> {
    if link.is_local() {
        return fetch_here(agent, download, into, target_os);
    }
    if let HostFetch::Done = on_host::fetch(link, download, into)? {
        return Ok(());
    }
    let staging = Staging::new(into)?;
    let here = Platform::current().map_or(Os::Linux, |platform| platform.os);
    fetch_here(agent, download, &staging.0, here)?;
    link.create_dir_all(into)?;
    for entry in fs::read_dir(&staging.0)? {
        link.upload(&entry?.path(), into)?;
    }
    on_host::mark_executables(link, into)
}

fn fetch_here(
    agent: &Agent,
    file: &Download<'_>,
    into: &Path,
    os: Os,
) -> Result<(), ExtensionError> {
    fs::create_dir_all(into)?;
    let downloaded = into.join(format!("{}{DOWNLOAD_SUFFIX}", file.name));
    let fetched = download(agent, file.url, file.digest, &downloaded)
        .and_then(|()| place(&downloaded, file.name, into, os));
    if downloaded.exists() {
        fs::remove_file(&downloaded)?;
    }
    fetched
}
