use krad_domain::JournalOffset;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const OUTPUT_FILE: &str = "output";
const ERRORS_FILE: &str = "errors";
const PID_FILE: &str = "pid";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Journal {
    dir: PathBuf,
}

impl Journal {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub(crate) fn exists(&self) -> bool {
        self.output_path().is_file()
    }

    pub(crate) fn create(&self) -> io::Result<(File, File)> {
        fs::create_dir_all(&self.dir)?;
        let output = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.output_path())?;
        let errors = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(ERRORS_FILE))?;
        Ok((output, errors))
    }

    pub(crate) fn record_pid(&self, pid: u32) -> io::Result<()> {
        fs::write(self.dir.join(PID_FILE), pid.to_string())
    }

    pub(crate) fn len(&self) -> io::Result<JournalOffset> {
        Ok(JournalOffset(fs::metadata(self.output_path())?.len()))
    }

    pub(crate) fn read(&self, from: JournalOffset, max: usize) -> io::Result<Vec<u8>> {
        read_from(&self.output_path(), from, max)
    }

    fn output_path(&self) -> PathBuf {
        self.dir.join(OUTPUT_FILE)
    }
}

pub(crate) fn append(file: &mut File, bytes: &[u8]) -> io::Result<()> {
    file.write_all(bytes)?;
    file.flush()
}

fn read_from(path: &Path, from: JournalOffset, max: usize) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(from.0))?;
    let mut bytes = Vec::with_capacity(max);
    file.take(max as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_journal_reads_back_from_any_offset() -> TestResult {
        let dir = std::env::temp_dir().join(format!("krad-journal-{}", std::process::id()));
        let journal = Journal::new(dir.clone());
        assert!(!journal.exists());
        let (mut output, _) = journal.create()?;
        append(&mut output, b"hello ")?;
        append(&mut output, b"world")?;
        assert!(journal.exists());
        assert_eq!(journal.len()?, JournalOffset(11));
        assert_eq!(journal.read(JournalOffset(6), 64)?, b"world");
        assert_eq!(journal.read(JournalOffset(0), 5)?, b"hello");
        assert_eq!(journal.read(JournalOffset(11), 64)?, b"");
        journal.record_pid(42)?;
        assert_eq!(fs::read_to_string(dir.join(PID_FILE))?, "42");
        fs::remove_dir_all(dir)?;
        Ok(())
    }
}
