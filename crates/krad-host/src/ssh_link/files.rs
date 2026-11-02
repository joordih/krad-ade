use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::SshLink;
use crate::command::{env_file_text, quote};
use crate::error::HostError;
use crate::mode::FileMode;
use crate::path::remote_path;

const ENV_FILE_PREFIX: &str = "env-";
const SCRIPT_FILE_PREFIX: &str = "run-";
const SCRIPT_SUFFIX: &str = ".sh";
const SELF_REMOVAL: &str = "rm -f --";

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

fn unique_name(prefix: &str, suffix: &str) -> String {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(NEXT_FILE.fetch_add(1, Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    format!("{prefix}{:016x}{suffix}", hasher.finish())
}

impl SshLink {
    /// # Errors
    pub fn write_env_file(&self, vars: &[(String, String)]) -> Result<PathBuf, HostError> {
        let dir = self.layout().ok_or(HostError::NotConnected)?.env_dir()?;
        self.create_dir_with_mode(&dir, FileMode::PRIVATE_DIR)?;
        let file = dir.join(unique_name(ENV_FILE_PREFIX, ""));
        self.write_with_mode(
            &file,
            env_file_text(vars)?.as_bytes(),
            FileMode::PRIVATE_FILE,
        )?;
        Ok(file)
    }

    pub(crate) fn stage_script(&self, script: &str) -> Result<String, HostError> {
        let dir = self.layout().ok_or(HostError::NotConnected)?.run_dir()?;
        self.create_dir_with_mode(&dir, FileMode::PRIVATE_DIR)?;
        let file = dir.join(unique_name(SCRIPT_FILE_PREFIX, SCRIPT_SUFFIX));
        let remote = remote_path(&file);
        let staged = format!("{SELF_REMOVAL} {}\n{script}\n", quote(&remote));
        self.write_with_mode(&file, staged.as_bytes(), FileMode::PRIVATE_FILE)?;
        tracing::debug!(bytes = script.len(), "staged a long remote script");
        Ok(remote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_files_never_share_a_name() {
        assert_ne!(unique_name("run-", ".sh"), unique_name("run-", ".sh"));
        assert!(unique_name("env-", "").starts_with("env-"));
    }
}
