use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

const MANIFEST_FILE: &str = "manifest.jsonl";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "before", rename_all = "snake_case")]
enum Before {
    Absent,
    Saved { blob: String },
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Kept {
    path: PathBuf,
    #[serde(flatten)]
    before: Before,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshots {
    dir: PathBuf,
}

impl Snapshots {
    pub(crate) const fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn turn_dir(&self, turn: usize) -> PathBuf {
        self.dir.join(turn.to_string())
    }

    fn turns_from(&self, first: usize) -> io::Result<Vec<usize>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut turns: Vec<usize> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
            .filter(|turn| *turn >= first)
            .collect();
        turns.sort_unstable();
        Ok(turns)
    }

    pub(crate) fn keep(&self, turn: usize, path: &Path) -> io::Result<()> {
        let dir = self.turn_dir(turn);
        let kept = manifest(&dir)?;
        if kept.iter().any(|kept| kept.path == path) {
            return Ok(());
        }
        fs::create_dir_all(&dir)?;
        let before = match fs::read(path) {
            Ok(bytes) => {
                let blob = kept.len().to_string();
                fs::write(dir.join(&blob), bytes)?;
                Before::Saved { blob }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Before::Absent,
            Err(error) => return Err(error),
        };
        let entry = Kept {
            path: path.to_path_buf(),
            before,
        };
        let mut line = serde_json::to_string(&entry).map_err(io::Error::other)?;
        line.push('\n');
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(MANIFEST_FILE))?
            .write_all(line.as_bytes())
    }

    pub(crate) fn restore_from(&self, first: usize) -> io::Result<()> {
        let mut earliest: HashMap<PathBuf, (PathBuf, Before)> = HashMap::new();
        for turn in self.turns_from(first)? {
            let dir = self.turn_dir(turn);
            for kept in manifest(&dir)? {
                earliest
                    .entry(kept.path)
                    .or_insert((dir.clone(), kept.before));
            }
        }
        for (path, (dir, before)) in earliest {
            match before {
                Before::Saved { blob } => {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(dir.join(blob), &path)?;
                }
                Before::Absent => match fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                },
            }
        }
        Ok(())
    }

    pub(crate) fn forget_from(&self, first: usize) -> io::Result<()> {
        for turn in self.turns_from(first)? {
            fs::remove_dir_all(self.turn_dir(turn))?;
        }
        Ok(())
    }
}

fn manifest(dir: &Path) -> io::Result<Vec<Kept>> {
    let file = match fs::File::open(dir.join(MANIFEST_FILE)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut kept = Vec::new();
    for line in BufReader::new(file).lines() {
        if let Ok(entry) = serde_json::from_str::<Kept>(&line?) {
            kept.push(entry);
        }
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::new_session_id;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir()
            .join("krad-engine-snapshots")
            .join(new_session_id().as_str());
        fs::create_dir_all(&dir).unwrap_or_default();
        dir
    }

    #[test]
    fn a_files_rewind_puts_back_what_each_file_held_before_the_turn() -> io::Result<()> {
        let root = scratch();
        let snapshots = Snapshots::new(root.join("snapshots"));
        let edited = root.join("edited.txt");
        let created = root.join("new").join("created.txt");
        let later = root.join("later.txt");
        fs::write(&edited, "first\n")?;
        fs::write(&later, "kept\n")?;
        snapshots.keep(0, &edited)?;
        fs::write(&edited, "turn 0\n")?;
        snapshots.keep(1, &edited)?;
        snapshots.keep(1, &created)?;
        fs::write(&edited, "turn 1\n")?;
        snapshots.keep(1, &edited)?;
        fs::create_dir_all(root.join("new"))?;
        fs::write(&created, "made\n")?;
        snapshots.keep(2, &later)?;
        fs::write(&later, "changed\n")?;
        snapshots.restore_from(1)?;
        snapshots.forget_from(1)?;
        assert_eq!(fs::read_to_string(&edited)?, "turn 0\n");
        assert_eq!(fs::read_to_string(&later)?, "kept\n");
        assert!(!created.exists());
        assert_eq!(snapshots.turns_from(0)?, [0]);
        fs::remove_dir_all(root)
    }
}
