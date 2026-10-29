use std::env;
use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Command;

const ICON: &str = "../../packaging/windows/krad-ade.ico";
const RESOURCE_COMPILER: &str = "windres";
const RESOURCE_COMPILER_VAR: &str = "WINDRES";
const COFF_OUTPUT_ARGS: [&str; 3] = ["-O", "coff", "-o"];
const MISSING_COMPILER: &str =
    "was not found, so the window icon is not embedded; set WINDRES to the resource compiler";
const ICON_RESOURCE_ID: u16 = 1;
const RESOURCE_SCRIPT: &str = "icon.rc";
const RESOURCE_OBJECT: &str = "icon.o";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-env-changed={RESOURCE_COMPILER_VAR}");
    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = env::var("CARGO_CFG_TARGET_ENV")?;
    if target_os != "windows" {
        return Ok(());
    }
    if target_env != "gnu" {
        println!("cargo:warning=the window icon is only embedded on the windows-gnu target");
        return Ok(());
    }
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let icon = fs::canonicalize(manifest_dir.join(ICON))?;
    let script = out_dir.join(RESOURCE_SCRIPT);
    let object = out_dir.join(RESOURCE_OBJECT);
    let icon_path = icon.display().to_string().replace('\\', "/");
    fs::write(
        &script,
        format!("{ICON_RESOURCE_ID} ICON \"{icon_path}\"\n"),
    )?;
    let compiler = env::var_os(RESOURCE_COMPILER_VAR).unwrap_or_else(|| RESOURCE_COMPILER.into());
    let status = match Command::new(&compiler)
        .arg(&script)
        .args(COFF_OUTPUT_ARGS)
        .arg(&object)
        .status()
    {
        Ok(status) => status,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            println!("cargo:warning={} {MISSING_COMPILER}", compiler.display());
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    if !status.success() {
        return Err(format!("{} failed with {status}", compiler.display()).into());
    }
    println!("cargo:rustc-link-arg-bins={}", object.display());
    Ok(())
}
