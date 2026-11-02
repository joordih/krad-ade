use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use super::wire::{FileKind, OpenFlags};
use super::{file_name, join, parent, Sftp, SftpError};

const TEMP_PREFIX: &str = ".";
const TEMP_MARKER: &str = ".krad-save-";
const TEMP_SUFFIX: &str = ".tmp";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    Replaced,
    InPlace,
}

#[derive(Clone, Copy, Debug)]
struct Existing {
    owner: Option<(u32, u32)>,
    mode: Option<u32>,
}

fn temp_name(target: &str) -> String {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(NEXT_TEMP.fetch_add(1, Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    join(
        parent(target),
        &format!(
            "{TEMP_PREFIX}{}{TEMP_MARKER}{:016x}{TEMP_SUFFIX}",
            file_name(target),
            hasher.finish()
        ),
    )
}

impl Sftp {
    fn real_target(&self, path: &str) -> Result<String, SftpError> {
        match self.link_attrs(path) {
            Ok(attrs) if attrs.kind() == FileKind::Symlink => self.realpath(path),
            _ => Ok(path.to_owned()),
        }
    }

    /// # Errors
    pub fn save(&self, path: &str, bytes: &[u8]) -> Result<Saved, SftpError> {
        let target = self.real_target(path)?;
        let existing = match self.attrs(&target) {
            Ok(attrs) => Some(attrs),
            Err(error) if error.is_not_found() => None,
            Err(error) => return Err(error),
        };
        if !self.posix_rename {
            self.write(&target, bytes)?;
            return Ok(Saved::InPlace);
        }
        let mode = existing.and_then(|attrs| attrs.mode());
        let temp = temp_name(&target);
        let handle = match self.open(&temp, OpenFlags::CREATE_EXCLUSIVE, mode) {
            Ok(handle) => handle,
            Err(SftpError::PermissionDenied(_)) => {
                self.write(&target, bytes)?;
                return Ok(Saved::InPlace);
            }
            Err(error) => return Err(error),
        };
        let written = self.write_handle(&handle, &temp, bytes);
        let closed = self.close(&handle, &temp);
        if let Err(error) = written.and(closed) {
            self.remove_file(&temp).ok();
            return Err(error);
        }
        let existing = existing.map(|attrs| Existing {
            owner: attrs.owner,
            mode,
        });
        match self.finish(&temp, &target, existing, bytes) {
            Ok(saved) => Ok(saved),
            Err(error) => {
                self.remove_file(&temp).ok();
                Err(error)
            }
        }
    }

    /// # Errors
    pub fn replace_from(
        &self,
        path: &str,
        source: &mut impl std::io::Read,
        mode: Option<u32>,
    ) -> Result<u64, SftpError> {
        let temp = temp_name(path);
        let handle = self.open(&temp, OpenFlags::CREATE_EXCLUSIVE, mode)?;
        let written = self.stream_into(&handle, &temp, source);
        let closed = self.close(&handle, &temp);
        let placed = written
            .and_then(|sent| closed.map(|()| sent))
            .and_then(|sent| {
                if !self.posix_rename && self.link_attrs(path).is_ok() {
                    self.remove_file(path)?;
                }
                self.rename(&temp, path).map(|()| sent)
            });
        if placed.is_err() {
            self.remove_file(&temp).ok();
        }
        placed
    }

    fn finish(
        &self,
        temp: &str,
        target: &str,
        existing: Option<Existing>,
        bytes: &[u8],
    ) -> Result<Saved, SftpError> {
        if let Some(Existing { owner, mode }) = existing {
            let created = self.attrs(temp)?;
            if owner.is_some() && created.owner != owner {
                self.remove_file(temp)?;
                self.write(target, bytes)?;
                return Ok(Saved::InPlace);
            }
            if let Some(mode) = mode {
                self.set_permissions(temp, mode)?;
            }
        }
        self.rename(temp, target)?;
        Ok(Saved::Replaced)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn temp_files_sit_hidden_next_to_their_target_and_never_repeat() {
        let first = temp_name("/srv/app/src/main.rs");
        assert!(first.starts_with("/srv/app/src/.main.rs.krad-save-"));
        assert!(Path::new(&first)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp")));
        assert_ne!(first, temp_name("/srv/app/src/main.rs"));
        assert_eq!(parent("/main.rs"), "/");
        assert_eq!(file_name("/srv/x/"), "x");
    }
}
