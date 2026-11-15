use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const WEB_DIST: &str = "../../web/dist";
const OUTPUT: &str = "web_assets.rs";

fn collect(root: &Path, dir: &Path, found: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, found)?;
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let name = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        found.push((name, fs::canonicalize(&path)?));
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let dist = manifest.join(WEB_DIST);
    println!("cargo::rerun-if-changed={}", dist.display());
    let mut found = Vec::new();
    if dist.is_dir() {
        collect(&dist, &dist, &mut found)?;
    }
    found.sort();
    let mut table = String::from("pub(crate) static WEB_ASSETS: &[(&str, &[u8])] = &[\n");
    for (name, path) in &found {
        writeln!(
            table,
            "    ({name:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .map_err(io::Error::other)?;
    }
    table.push_str("];\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap_or_default());
    fs::write(out.join(OUTPUT), table)
}
