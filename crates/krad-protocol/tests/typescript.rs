#![cfg(feature = "ts")]

use krad_protocol::Message;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use ts_rs::{Config, TS};

const BINDINGS: &str = "../../web/src/protocol";
const WRITE_VARIABLE: &str = "KRAD_WRITE_TS";
const LARGE_INT: &str = "number";
const STALE: &str = "web/src/protocol is out of date: run `KRAD_WRITE_TS=1 cargo test -p krad-protocol --features ts --test typescript`";

fn files(root: &Path) -> io::Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut found = BTreeMap::new();
    if !root.is_dir() {
        return Ok(found);
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                found.insert(relative.to_path_buf(), fs::read(&path)?);
            }
        }
    }
    Ok(found)
}

fn bindings() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(BINDINGS)
}

#[test]
fn the_web_bindings_match_the_protocol() -> Result<(), Box<dyn Error>> {
    let fresh = std::env::temp_dir().join(format!("krad-ts-{}", std::process::id()));
    if fresh.exists() {
        fs::remove_dir_all(&fresh)?;
    }
    let config = Config::new().with_large_int(LARGE_INT).with_out_dir(&fresh);
    Message::export_all(&config)?;
    let generated = files(&fresh)?;
    if std::env::var_os(WRITE_VARIABLE).is_some() {
        let target = bindings();
        if target.exists() {
            fs::remove_dir_all(&target)?;
        }
        for (relative, bytes) in &generated {
            let path = target.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, bytes)?;
        }
    } else if files(&bindings())? != generated {
        fs::remove_dir_all(&fresh)?;
        return Err(STALE.into());
    }
    fs::remove_dir_all(&fresh)?;
    Ok(())
}
