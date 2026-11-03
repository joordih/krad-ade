use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::time::Duration;
use ureq::Agent;

use crate::error::ExtensionError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const DOWNLOAD_CHUNK: usize = 64 * 1024;
const METADATA_LIMIT: u64 = 4 * 1024 * 1024;
const DOWNLOAD_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const USER_AGENT_HEADER: &str = "User-Agent";
const USER_AGENT: &str = concat!("krad-ade/", env!("CARGO_PKG_VERSION"));
const GITHUB_API: &str = "https://api.github.com/repos";
const LATEST_RELEASE: &str = "releases/latest";
const RELEASE_BY_TAG: &str = "releases/tags";
const SHA256_PREFIX: &str = "sha256:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sha256Digest(String);

impl Sha256Digest {
    pub(crate) fn new(hex: &str) -> Self {
        Self(hex.trim().to_ascii_lowercase())
    }

    pub(crate) fn hex(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Release {
    pub(crate) assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReleaseAsset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

impl ReleaseAsset {
    pub(crate) fn sha256(&self) -> Option<Sha256Digest> {
        self.digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix(SHA256_PREFIX))
            .map(Sha256Digest::new)
    }
}

pub(crate) fn agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_per_read(Some(READ_TIMEOUT))
        .build()
        .new_agent()
}

pub(crate) fn fetch_text(agent: &Agent, url: &str) -> Result<String, ExtensionError> {
    let mut response = agent
        .get(url)
        .header(USER_AGENT_HEADER, USER_AGENT)
        .call()?;
    Ok(response
        .body_mut()
        .with_config()
        .limit(METADATA_LIMIT)
        .read_to_string()?)
}

pub(crate) fn latest_release(agent: &Agent, repository: &str) -> Result<Release, ExtensionError> {
    let text = fetch_text(
        agent,
        &format!("{GITHUB_API}/{repository}/{LATEST_RELEASE}"),
    )?;
    Ok(serde_json::from_str(&text)?)
}

pub(crate) fn release_by_tag(
    agent: &Agent,
    repository: &str,
    tag: &str,
) -> Result<Release, ExtensionError> {
    let text = fetch_text(
        agent,
        &format!("{GITHUB_API}/{repository}/{RELEASE_BY_TAG}/{tag}"),
    )?;
    Ok(serde_json::from_str(&text)?)
}

pub(crate) fn download(
    agent: &Agent,
    url: &str,
    expected: Option<&Sha256Digest>,
    to: &Path,
) -> Result<(), ExtensionError> {
    let mut response = agent
        .get(url)
        .header(USER_AGENT_HEADER, USER_AGENT)
        .call()?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(DOWNLOAD_LIMIT)
        .reader();
    let mut file = File::create(to)?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0; DOWNLOAD_CHUNK];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
        file.write_all(&chunk[..read])?;
    }
    file.flush()?;
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = Sha256Digest::new(&format!("{:x}", hasher.finalize()));
    if actual == *expected {
        return Ok(());
    }
    std::fs::remove_file(to)?;
    Err(ExtensionError::Digest {
        url: url.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_sha256_digests_are_read_and_case_does_not_matter() {
        let asset = |digest: Option<&str>| ReleaseAsset {
            name: String::new(),
            browser_download_url: String::new(),
            digest: digest.map(str::to_owned),
        };
        assert_eq!(
            asset(Some("sha256:ABCDEF")).sha256(),
            Some(Sha256Digest::new("abcdef"))
        );
        assert_eq!(asset(Some("sha512:abcdef")).sha256(), None);
        assert_eq!(asset(None).sha256(), None);
    }
}
