use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use krad_domain::InstallId;
use std::path::{Path, PathBuf};

use super::super::clients::ensure_private_dir;
use super::{events_file, EVENTS_DIR};
use crate::layout::HostLayout;
use crate::mode::FileMode;

const QUEUE_LIMIT: u64 = 256 * 1024;
const QUEUE_KEPT: usize = 128 * 1024;
const LINE_LIMIT: usize = 16 * 1024;
const LINE_END: u8 = b'\n';
const TRIM_EXTENSION: &str = "trim";

#[must_use]
pub fn home_runner_dir() -> Option<PathBuf> {
    let Some(home) = std::env::home_dir() else {
        tracing::debug!("no HOME, so there is no runner folder");
        return None;
    };
    Some(HostLayout::new(&home, None).runner_dir())
}

pub fn queue_events(runner_dir: &Path, lines: &[(InstallId, String)]) {
    if lines.is_empty() {
        return;
    }
    if let Err(error) = ensure_private_dir(&runner_dir.join(EVENTS_DIR)) {
        tracing::debug!(%error, "the event queue folder could not be made");
        return;
    }
    for (install, line) in lines {
        if line.len() > LINE_LIMIT || line.bytes().any(|byte| byte == LINE_END) {
            tracing::debug!(
                bytes = line.len(),
                "an event too long for the queue was dropped"
            );
            continue;
        }
        if let Err(error) = append_bounded(&events_file(runner_dir, install), line) {
            tracing::debug!(%error, %install, "an event could not be queued");
        }
    }
}

fn private_file(path: &Path, append: bool) -> io::Result<fs::File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .mode(FileMode::PRIVATE_FILE.0)
        .open(path)
}

fn append_bounded(file: &Path, line: &str) -> io::Result<()> {
    let mut queue = private_file(file, true)?;
    let mut entry = Vec::with_capacity(line.len() + 1);
    entry.extend_from_slice(line.as_bytes());
    entry.push(LINE_END);
    queue.write_all(&entry)?;
    if queue.metadata()?.len() > QUEUE_LIMIT {
        trim(file)?;
    }
    Ok(())
}

fn trim(file: &Path) -> io::Result<()> {
    let text = fs::read(file)?;
    let temp = file.with_extension(TRIM_EXTENSION);
    private_file(&temp, false)?.write_all(newest_lines(&text, QUEUE_KEPT))?;
    fs::rename(temp, file)
}

fn newest_lines(text: &[u8], budget: usize) -> &[u8] {
    let Some(start) = text.len().checked_sub(budget).filter(|start| *start > 0) else {
        return text;
    };
    if text.get(start - 1) == Some(&LINE_END) {
        return &text[start..];
    }
    text[start..]
        .iter()
        .position(|byte| *byte == LINE_END)
        .map_or(&[], |end| &text[start + end + 1..])
}

#[cfg(test)]
mod tests {
    use super::super::super::clients::{pair, paired_installs};
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    struct Root(std::path::PathBuf);

    impl Drop for Root {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    fn queue_event_in(runner_dir: &Path, line: &str) {
        let lines: Vec<(InstallId, String)> = paired_installs(runner_dir)
            .into_iter()
            .map(|install| (install, line.to_owned()))
            .collect();
        queue_events(runner_dir, &lines);
    }

    fn root(name: &str) -> Root {
        Root(std::env::temp_dir().join(format!("krad-queue-{name}-{}", std::process::id())))
    }

    #[test]
    fn every_paired_installation_gets_its_own_copy_of_the_event() -> TestResult {
        let root = root("fan-out");
        queue_event_in(&root.0, "{\"lost\":true}");
        assert!(!root.0.join(EVENTS_DIR).exists());
        let first: InstallId = "0123456789abcdef".parse()?;
        let second: InstallId = "fedcba9876543210".parse()?;
        pair(&root.0, &first)?;
        pair(&root.0, &second)?;
        queue_event_in(&root.0, "{\"n\":1}");
        queue_event_in(&root.0, "{\"n\":2}");
        for install in [&first, &second] {
            let file = events_file(&root.0, install);
            assert_eq!(fs::read_to_string(&file)?, "{\"n\":1}\n{\"n\":2}\n");
            assert_eq!(fs::metadata(&file)?.permissions().mode() & 0o777, 0o600);
        }
        queue_event_in(&root.0, "two\nlines");
        assert_eq!(
            fs::read_to_string(events_file(&root.0, &first))?,
            "{\"n\":1}\n{\"n\":2}\n"
        );
        Ok(())
    }

    #[test]
    fn a_full_queue_keeps_only_its_newest_whole_lines() -> TestResult {
        let root = root("bound");
        let install: InstallId = "0123456789abcdef".parse()?;
        pair(&root.0, &install)?;
        let line = "x".repeat(1000);
        let lines = usize::try_from(QUEUE_LIMIT)? / line.len() + 1;
        for _ in 0..lines {
            queue_event_in(&root.0, &line);
        }
        queue_event_in(&root.0, "last");
        let kept = fs::read_to_string(events_file(&root.0, &install))?;
        assert!(kept.len() <= QUEUE_KEPT + line.len() + 1);
        assert!(kept.ends_with("last\n"));
        assert!(kept.lines().all(|kept| kept == line || kept == "last"));
        Ok(())
    }

    #[test]
    fn the_newest_lines_start_at_a_line_boundary() {
        assert_eq!(newest_lines(b"aa\nbb\ncc\n", 20), b"aa\nbb\ncc\n");
        assert_eq!(newest_lines(b"aa\nbb\ncc\n", 6), b"bb\ncc\n");
        assert_eq!(newest_lines(b"aa\nbb\ncc\n", 5), b"cc\n");
        assert_eq!(newest_lines(b"aaaaaa\n", 3), b"");
    }
}
