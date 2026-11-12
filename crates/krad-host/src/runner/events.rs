#[cfg(unix)]
mod queue;

#[cfg(unix)]
pub use queue::{home_runner_dir, queue_events};

use krad_domain::InstallId;
use std::path::{Path, PathBuf};

use crate::command::HostCommand;
use crate::path::remote_path;

const EVENTS_DIR: &str = "events";
const EVENTS_EXTENSION: &str = "jsonl";
const TAKING_EXTENSION: &str = "taking";
const WEBHOOKS_DIR: &str = "webhooks";
const WEBHOOKS_EXTENSION: &str = "json";
const TAKE_SCRIPT: &str = r#"q="$1"; t="$2"; if [ -f "$q" ] && mv -f "$q" "$q.$$"; then cat "$q.$$" >> "$t" && rm -f "$q.$$"; fi; if [ -f "$t" ]; then cat "$t" && rm -f "$t"; fi"#;

#[must_use]
pub fn events_file(runner_dir: &Path, install: &InstallId) -> PathBuf {
    runner_dir
        .join(EVENTS_DIR)
        .join(install.as_str())
        .with_extension(EVENTS_EXTENSION)
}

#[must_use]
pub fn webhooks_dir(runner_dir: &Path) -> PathBuf {
    runner_dir.join(WEBHOOKS_DIR)
}

#[must_use]
pub fn webhooks_file(runner_dir: &Path, install: &InstallId) -> PathBuf {
    webhooks_dir(runner_dir)
        .join(install.as_str())
        .with_extension(WEBHOOKS_EXTENSION)
}

#[must_use]
pub fn take_events_command(runner_dir: &Path, install: &InstallId) -> HostCommand {
    let queue = events_file(runner_dir, install);
    let taking = queue.with_extension(TAKING_EXTENSION);
    let mut command = HostCommand::posix_shell(TAKE_SCRIPT);
    command.args([remote_path(&queue), remote_path(&taking)]);
    command
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::link::HostLink;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn take(root: &Path, install: &InstallId) -> Result<String, Box<dyn std::error::Error>> {
        let output = HostLink::Local.output(&take_events_command(root, install))?;
        assert!(output.status.success());
        Ok(String::from_utf8(output.stdout)?)
    }

    #[test]
    fn taking_the_queue_reads_it_once_and_leaves_it_empty() -> TestResult {
        let root = std::env::temp_dir().join(format!("krad-take-{}", std::process::id()));
        let install: InstallId = "0123456789abcdef".parse()?;
        let queue = events_file(&root, &install);
        assert_eq!(
            queue,
            root.join("events/0123456789abcdef.jsonl"),
            "the queue sits under the runner's events folder"
        );
        assert_eq!(take(&root, &install)?, "");
        std::fs::create_dir_all(queue.parent().ok_or("no parent")?)?;
        std::fs::write(queue.with_extension(TAKING_EXTENSION), "{\"left\":1}\n")?;
        std::fs::write(&queue, "{\"a\":1}\n{\"b\":2}\n")?;
        assert_eq!(
            take(&root, &install)?,
            "{\"left\":1}\n{\"a\":1}\n{\"b\":2}\n"
        );
        assert!(!queue.exists());
        assert_eq!(take(&root, &install)?, "");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
