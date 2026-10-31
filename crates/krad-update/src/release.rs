use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::time::Duration;

use ureq::Agent;

use crate::error::UpdateError;
use crate::installation::{AfterInstall, Installation};
use crate::manifest::{ReleaseBuild, ReleaseManifest};
use crate::signature::{ReleaseKey, ReleaseSignature};
use crate::version::AppVersion;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const LATEST_DOWNLOAD_PATH: &str = "releases/latest/download";
const RELEASE_DOWNLOAD_PATH: &str = "releases/download";
const RELEASE_PAGE_PATH: &str = "releases/tag";
const MANIFEST_FILE: &str = "latest.json";
const MANIFEST_SIGNATURE_FILE: &str = "latest.json.sig";
const MANIFEST_LIMIT: u64 = 1024 * 1024;
const SIGNATURE_LIMIT: u64 = 4 * 1024;
const DOWNLOAD_CHUNK: usize = 64 * 1024;
const END_OF_BODY_READ: u64 = 1;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct AvailableUpdate {
    pub version: AppVersion,
    pub notes: String,
    pub page: String,
    build: ReleaseBuild,
    installation: Installation,
}

#[derive(Clone, Debug)]
pub struct StagedUpdate {
    pub version: AppVersion,
    payload: PathBuf,
    installation: Installation,
}

fn agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_per_read(Some(READ_TIMEOUT))
        .build()
        .new_agent()
}

fn latest_file_url(file: &str) -> String {
    format!("{REPOSITORY}/{LATEST_DOWNLOAD_PATH}/{file}")
}

fn release_file_url(version: AppVersion, file: &str) -> String {
    format!(
        "{REPOSITORY}/{RELEASE_DOWNLOAD_PATH}/{}/{file}",
        version.tag()
    )
}

fn release_page_url(version: AppVersion) -> String {
    format!("{REPOSITORY}/{RELEASE_PAGE_PATH}/{}", version.tag())
}

fn fetch(agent: &Agent, url: &str, limit: u64) -> Result<Vec<u8>, UpdateError> {
    let mut response = agent.get(url).call()?;
    Ok(response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()?)
}

fn stream_download(
    agent: &Agent,
    url: &str,
    size: u64,
    mut sink: impl FnMut(&[u8]) -> Result<(), UpdateError>,
) -> Result<(), UpdateError> {
    let mut response = agent.get(url).call()?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(size.saturating_add(END_OF_BODY_READ))
        .reader();
    let mut chunk = vec![0; DOWNLOAD_CHUNK];
    let mut received: u64 = 0;
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        sink(&chunk[..read])?;
        received += read as u64;
    }
    if received != size {
        return Err(UpdateError::Size {
            expected: size,
            received,
        });
    }
    Ok(())
}

fn verified_manifest(agent: &Agent) -> Result<ReleaseManifest, UpdateError> {
    let manifest = fetch(agent, &latest_file_url(MANIFEST_FILE), MANIFEST_LIMIT)?;
    let signature = fetch(
        agent,
        &latest_file_url(MANIFEST_SIGNATURE_FILE),
        SIGNATURE_LIMIT,
    )?;
    let signature: ReleaseSignature = String::from_utf8_lossy(&signature).parse()?;
    ReleaseKey::embedded()?.verify(&manifest, &signature)?;
    Ok(serde_json::from_slice(&manifest)?)
}

fn newer_build(
    manifest: ReleaseManifest,
    running: AppVersion,
    installation: &Installation,
) -> Result<Option<AvailableUpdate>, UpdateError> {
    if manifest.version <= running {
        return Ok(None);
    }
    let build = manifest
        .builds
        .get(&installation.target)
        .cloned()
        .ok_or(UpdateError::NoBuild)?;
    Ok(Some(AvailableUpdate {
        version: manifest.version,
        notes: manifest.notes,
        page: release_page_url(manifest.version),
        build,
        installation: installation.clone(),
    }))
}

/// # Errors
///
/// [`UpdateError`] when GitHub can't be reached, the manifest's signature
/// doesn't match the release key or the release has no build for this platform.
pub fn latest_update(installation: &Installation) -> Result<Option<AvailableUpdate>, UpdateError> {
    let manifest = verified_manifest(&agent())?;
    newer_build(manifest, AppVersion::running()?, installation)
}

impl AvailableUpdate {
    /// # Errors
    ///
    /// [`UpdateError`] when the download fails, its size or signature doesn't
    /// match the manifest, or it can't be unpacked next to the installation.
    pub fn download(self) -> Result<StagedUpdate, UpdateError> {
        let staging = self.installation.fresh_staging()?;
        let archive = staging.join(self.build.file.as_ref());
        let key = ReleaseKey::embedded()?;
        let mut verifier = key.stream(&self.build.signature)?;
        let url = release_file_url(self.version, self.build.file.as_ref());
        let mut file = File::create(&archive)?;
        stream_download(&agent(), &url, self.build.size, |bytes| {
            verifier.update(bytes);
            Ok(file.write_all(bytes)?)
        })?;
        verifier.finalize()?;
        file.sync_all()?;
        let payload = self.installation.unpack(&archive)?;
        Ok(StagedUpdate {
            version: self.version,
            payload,
            installation: self.installation,
        })
    }
}

impl StagedUpdate {
    /// # Errors
    ///
    /// [`UpdateError`] when the installation can't be replaced or the
    /// relaunch can't be scheduled.
    pub fn install(&self, after: AfterInstall) -> Result<(), UpdateError> {
        self.installation.install(&self.payload, after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ReleaseTarget;
    use crate::signature::tests::SIGNATURE;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn manifest(version: &str) -> Result<ReleaseManifest, serde_json::Error> {
        serde_json::from_value(serde_json::json!({
            "version": version,
            "builds": {
                "aarch64-macos": { "file": "app.tar.gz", "size": 1, "signature": SIGNATURE }
            }
        }))
    }

    fn installation(target: ReleaseTarget) -> Installation {
        Installation {
            root: PathBuf::from("install"),
            staging: PathBuf::from("staging"),
            target,
        }
    }

    #[test]
    fn offers_only_a_strictly_newer_version() -> TestResult {
        let running: AppVersion = "0.2.0".parse()?;
        let mac = installation(ReleaseTarget::MacosAarch64);
        for older_or_same in ["0.1.9", "0.2.0"] {
            assert!(newer_build(manifest(older_or_same)?, running, &mac)?.is_none());
        }
        let update = newer_build(manifest("0.2.1")?, running, &mac)?.ok_or("no update offered")?;
        assert_eq!(update.page, format!("{REPOSITORY}/releases/tag/v0.2.1"));
        Ok(())
    }

    #[test]
    fn a_newer_release_without_this_platform_is_an_error() -> TestResult {
        let running: AppVersion = "0.2.0".parse()?;
        let linux = installation(ReleaseTarget::LinuxX86_64);
        assert!(matches!(
            newer_build(manifest("0.3.0")?, running, &linux),
            Err(UpdateError::NoBuild)
        ));
        Ok(())
    }

    fn serve_once(body: &'static [u8]) -> Result<String, std::io::Error> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let url = format!("http://{}/build", listener.local_addr()?);
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0; 1024];
            stream.read(&mut request).unwrap_or_default();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap_or_default();
            stream.write_all(body).unwrap_or_default();
        });
        Ok(url)
    }

    fn download(body: &'static [u8], size: u64) -> Result<Vec<u8>, UpdateError> {
        let url = serve_once(body)?;
        let mut received = Vec::new();
        stream_download(&agent(), &url, size, |bytes| {
            received.extend_from_slice(bytes);
            Ok(())
        })?;
        Ok(received)
    }

    #[test]
    fn a_build_of_exactly_its_manifest_size_downloads_whole() -> TestResult {
        let body = b"build bytes";
        assert_eq!(download(body, body.len() as u64)?, body);
        Ok(())
    }

    #[test]
    fn a_build_larger_or_smaller_than_its_manifest_size_is_refused() {
        let body = b"build bytes";
        let size = body.len() as u64;
        assert!(download(body, size - 2).is_err());
        assert!(matches!(
            download(body, size + 2),
            Err(UpdateError::Size { expected, received }) if expected == size + 2 && received == size
        ));
    }

    #[test]
    fn downloads_come_from_the_release_tag() -> TestResult {
        let version: AppVersion = "1.2.3".parse()?;
        assert_eq!(
            release_file_url(version, "a.tar.gz"),
            format!("{REPOSITORY}/releases/download/v1.2.3/a.tar.gz")
        );
        Ok(())
    }
}
