use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::error::HostError;
use crate::link::HostLink;

const CANCELLED_TEXT: &str = "the upload was cancelled";
const LOCAL_TEMP_PREFIX: &str = ".";
const LOCAL_TEMP_MARKER: &str = ".krad-upload-";

#[derive(Debug, Default)]
pub struct UploadProgress {
    sent: AtomicU64,
    total: AtomicU64,
    cancelled: AtomicBool,
}

impl UploadProgress {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    #[must_use]
    pub fn sent(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn check(&self) -> Result<(), HostError> {
        if self.is_cancelled() {
            return Err(HostError::Io(io::Error::new(
                io::ErrorKind::Interrupted,
                CANCELLED_TEXT,
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Existing {
    Replace,
    Skip,
}

struct Counted<'a, R> {
    inner: R,
    progress: &'a UploadProgress,
}

impl<R: Read> Read for Counted<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.progress.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, CANCELLED_TEXT));
        }
        let read = self.inner.read(buf)?;
        self.progress
            .sent
            .fetch_add(u64::try_from(read).unwrap_or(u64::MAX), Ordering::Relaxed);
        Ok(read)
    }
}

#[must_use]
pub fn local_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    fs::read_dir(path).map_or(0, |entries| {
        entries
            .flatten()
            .map(|entry| local_size(&entry.path()))
            .sum()
    })
}

impl HostLink {
    /// # Errors
    pub fn upload(&self, local: &Path, into: &Path) -> Result<PathBuf, HostError> {
        self.upload_with(local, into, &UploadProgress::default(), Existing::Replace)
    }

    /// # Errors
    pub fn upload_with(
        &self,
        local: &Path,
        into: &Path,
        progress: &UploadProgress,
        existing: Existing,
    ) -> Result<PathBuf, HostError> {
        let name = local.file_name().ok_or_else(|| {
            HostError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the dropped item has no name",
            ))
        })?;
        progress
            .total
            .fetch_add(local_size(local), Ordering::Relaxed);
        let target = into.join(name);
        self.upload_to(local, &target, progress, existing)?;
        Ok(target)
    }

    fn upload_to(
        &self,
        local: &Path,
        target: &Path,
        progress: &UploadProgress,
        existing: Existing,
    ) -> Result<(), HostError> {
        progress.check()?;
        let metadata = fs::symlink_metadata(local)?;
        if metadata.is_dir() {
            self.create_dir_all(target)?;
            for entry in fs::read_dir(local)? {
                let entry = entry?;
                self.upload_to(
                    &entry.path(),
                    &target.join(entry.file_name()),
                    progress,
                    existing,
                )?;
            }
            return Ok(());
        }
        if existing == Existing::Skip && self.symlink_metadata(target).is_ok() {
            progress.sent.fetch_add(metadata.len(), Ordering::Relaxed);
            return Ok(());
        }
        if metadata.is_symlink() {
            if self.symlink_metadata(target).is_ok() {
                self.remove_file(target)?;
            }
            return self.symlink(&fs::read_link(local)?, target);
        }
        if metadata.is_file() {
            let mode = crate::local::executable_mode(&metadata);
            let mut source = Counted {
                inner: fs::File::open(local)?,
                progress,
            };
            match self {
                Self::Local => replace_local(&mut source, target)?,
                Self::Ssh(ssh) => {
                    let remote = crate::remote_path(target);
                    ssh.with_bulk(|sftp| sftp.replace_from(&remote, &mut source, mode))?;
                }
            }
            if let Some(mode) = mode {
                self.set_permissions(target, crate::FileMode(mode))?;
            }
        }
        Ok(())
    }
}

fn replace_local(source: &mut impl Read, target: &Path) -> Result<(), HostError> {
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let temp = target.with_file_name(format!(
        "{LOCAL_TEMP_PREFIX}{name}{LOCAL_TEMP_MARKER}{}",
        std::process::id()
    ));
    let placed = fs::File::create_new(&temp)
        .and_then(|mut file| io::copy(source, &mut file))
        .and_then(|_| fs::rename(&temp, target));
    if let Err(error) = placed {
        fs::remove_file(&temp).ok();
        return Err(HostError::Io(error));
    }
    Ok(())
}

impl HostLink {
    /// # Errors
    pub fn symlink(&self, target: &Path, link: &Path) -> Result<(), HostError> {
        match self {
            Self::Local => Ok(crate::local::symlink(target, link)?),
            Self::Ssh(ssh) => ssh.with_sftp(|sftp| {
                sftp.symlink(&crate::remote_path(target), &crate::remote_path(link))
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn progress_counts_every_byte_and_skipping_keeps_existing_files() -> TestResult {
        let base = std::env::temp_dir().join(format!("krad-host-progress-{}", std::process::id()));
        let source = base.join("from").join("data");
        fs::create_dir_all(&source)?;
        fs::write(source.join("a.bin"), vec![1_u8; 1000])?;
        fs::write(source.join("b.bin"), vec![2_u8; 24])?;
        let into = base.join("to");
        fs::create_dir_all(into.join("data"))?;
        fs::write(into.join("data").join("a.bin"), "kept")?;
        let progress = UploadProgress::default();
        HostLink::Local.upload_with(&source, &into, &progress, Existing::Skip)?;
        assert_eq!(progress.total(), 1024);
        assert_eq!(progress.sent(), 1024);
        assert_eq!(fs::read_to_string(into.join("data").join("a.bin"))?, "kept");
        assert_eq!(fs::read(into.join("data").join("b.bin"))?.len(), 24);
        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn a_cancelled_upload_stops_before_copying() -> TestResult {
        let base = std::env::temp_dir().join(format!("krad-host-cancel-{}", std::process::id()));
        let source = base.join("big.bin");
        fs::create_dir_all(&base)?;
        fs::write(&source, vec![0_u8; 64])?;
        let into = base.join("to");
        fs::create_dir_all(&into)?;
        let progress = UploadProgress::default();
        progress.cancel();
        assert!(HostLink::Local
            .upload_with(&source, &into, &progress, Existing::Replace)
            .is_err());
        assert!(!into.join("big.bin").exists());
        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn a_cancelled_replace_keeps_the_original() -> TestResult {
        let base = std::env::temp_dir().join(format!("krad-host-keep-{}", std::process::id()));
        let source = base.join("config.yml");
        fs::create_dir_all(base.join("to"))?;
        fs::write(&source, "new")?;
        let target = base.join("to").join("config.yml");
        fs::write(&target, "original")?;
        let progress = UploadProgress::default();
        progress.cancel();
        assert!(HostLink::Local
            .upload_with(&source, &base.join("to"), &progress, Existing::Replace)
            .is_err());
        assert_eq!(fs::read_to_string(&target)?, "original");
        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn replacing_a_link_replaces_the_link_not_what_it_points_to() -> TestResult {
        let base = std::env::temp_dir().join(format!("krad-host-link-{}", std::process::id()));
        fs::create_dir_all(base.join("to"))?;
        let outside = base.join("outside.txt");
        fs::write(&outside, "elsewhere")?;
        std::os::unix::fs::symlink(&outside, base.join("to").join("a.txt"))?;
        let source = base.join("a.txt");
        fs::write(&source, "dropped")?;
        HostLink::Local.upload(&source, &base.join("to"))?;
        assert_eq!(fs::read_to_string(&outside)?, "elsewhere");
        let landed = base.join("to").join("a.txt");
        assert!(!fs::symlink_metadata(&landed)?.is_symlink());
        assert_eq!(fs::read_to_string(landed)?, "dropped");
        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn a_dropped_folder_lands_whole_inside_the_target() -> TestResult {
        let base = std::env::temp_dir().join(format!("krad-host-upload-{}", std::process::id()));
        let source = base.join("from").join("assets");
        fs::create_dir_all(source.join("icons"))?;
        fs::write(source.join("icons").join("a.svg"), "<svg/>")?;
        fs::write(source.join("readme.txt"), "hi")?;
        let into = base.join("to");
        fs::create_dir_all(&into)?;
        let landed = HostLink::Local.upload(&source, &into)?;
        assert_eq!(landed, into.join("assets"));
        assert_eq!(
            fs::read_to_string(landed.join("icons").join("a.svg"))?,
            "<svg/>"
        );
        assert_eq!(fs::read_to_string(landed.join("readme.txt"))?, "hi");
        fs::remove_dir_all(base)?;
        Ok(())
    }
}
