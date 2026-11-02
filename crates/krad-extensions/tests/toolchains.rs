use krad_domain::{
    ProjectFolder, ToolchainFamilyId, ToolchainKind, ToolchainLocation, VersionRequirement,
};
use krad_extensions::{
    discover, entered_toolchain, project_folder_for, DiscoveryRequest, LanguageTools,
};
use krad_host::HostLink;
use std::path::{Path, PathBuf};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const TIMEOUT: Duration = Duration::from_secs(20);

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn scratch(name: &str) -> Result<Scratch, Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("krad-toolchains-{name}-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(Scratch(dir.canonicalize()?))
}

#[cfg(unix)]
fn fake_python(venv: &Path, version: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(venv.join("bin"))?;
    std::fs::write(venv.join("pyvenv.cfg"), format!("version = {version}\n"))?;
    let python = venv.join("bin/python");
    std::fs::write(&python, format!("#!/bin/sh\necho 'Python {version}'\n"))?;
    std::fs::set_permissions(&python, std::fs::Permissions::from_mode(0o755))
}

fn tools(config: &Path) -> Result<LanguageTools, Box<dyn std::error::Error>> {
    Ok(LanguageTools::load(config, &[])?)
}

#[cfg(unix)]
#[test]
fn a_project_folders_own_venv_is_found_and_picked_first() -> TestResult {
    let root = scratch("python")?;
    let repo = root.0.join("repo");
    std::fs::create_dir_all(repo.join("backend/app"))?;
    std::fs::write(
        repo.join("backend/pyproject.toml"),
        "[project]\nname = \"api\"\n",
    )?;
    std::fs::write(repo.join("backend/.python-version"), "3.12\n")?;
    fake_python(&repo.join("backend/.venv"), "3.12.4")?;
    fake_python(&repo.join(".venv"), "3.11.9")?;

    let tools = tools(&root.0.join("config"))?;
    let family = tools
        .toolchain_family(&ToolchainFamilyId::new("python"))
        .ok_or("no python family")?;
    let link = HostLink::Local;
    let folder = project_folder_for(&link, family, &repo, &repo.join("backend/app/main.py"))?;
    assert_eq!(folder.as_path(), Path::new("backend"));

    let found = discover(
        family,
        tools.shared_locators(),
        &DiscoveryRequest {
            link: &link,
            group_root: &repo,
            project_folder: &folder,
            home: None,
            host_cli: None,
            timeout: TIMEOUT,
        },
    );
    assert_eq!(found.hints.version, VersionRequirement::parse("3.12"));
    let best = found.best().ok_or("no toolchain found")?;
    assert_eq!(best.kind, ToolchainKind::new("venv"));
    assert_eq!(
        best.prefix.as_deref(),
        Some(repo.join("backend/.venv").as_path())
    );
    assert_eq!(
        best.version.as_ref().map(ToString::to_string).as_deref(),
        Some("3.12.4")
    );
    assert!(found
        .candidates
        .iter()
        .any(|toolchain| toolchain.prefix.as_deref() == Some(repo.join(".venv").as_path())));
    Ok(())
}

#[test]
fn the_workspace_typescript_wins_over_the_bundled_one() -> TestResult {
    let root = scratch("typescript")?;
    let repo = root.0.join("web");
    let lib = repo.join("node_modules/typescript/lib");
    std::fs::create_dir_all(&lib)?;
    std::fs::write(lib.join("tsserver.js"), "")?;
    std::fs::write(
        repo.join("node_modules/typescript/package.json"),
        r#"{"name":"typescript","version":"5.6.3"}"#,
    )?;
    std::fs::write(repo.join("package.json"), "{}")?;

    let tools = tools(&root.0.join("config"))?;
    let family = tools
        .toolchain_family(&ToolchainFamilyId::new("typescript"))
        .ok_or("no typescript family")?;
    let link = HostLink::Local;
    let found = discover(
        family,
        tools.shared_locators(),
        &DiscoveryRequest {
            link: &link,
            group_root: &repo,
            project_folder: &ProjectFolder::root(),
            home: None,
            host_cli: None,
            timeout: TIMEOUT,
        },
    );
    let best = found.best().ok_or("no toolchain found")?;
    assert_eq!(best.location, ToolchainLocation::Path { path: lib });
    assert_eq!(
        best.version.as_ref().map(ToString::to_string).as_deref(),
        Some("5.6.3")
    );
    assert_eq!(
        found.candidates.last().map(|toolchain| &toolchain.location),
        Some(&ToolchainLocation::Bundled)
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_entered_venv_folder_or_interpreter_is_probed_for_its_version() -> TestResult {
    let root = scratch("entered")?;
    let venv = root.0.join("envs/ml");
    fake_python(&venv, "3.10.14")?;
    std::fs::create_dir_all(root.0.join("empty"))?;
    let tools = tools(&root.0.join("config"))?;
    let family = tools
        .toolchain_family(&ToolchainFamilyId::new("python"))
        .ok_or("no python family")?;
    let link = HostLink::Local;
    let request = DiscoveryRequest {
        link: &link,
        group_root: &root.0,
        project_folder: &ProjectFolder::root(),
        home: None,
        host_cli: None,
        timeout: TIMEOUT,
    };

    let from_folder = entered_toolchain(family, tools.shared_locators(), &request, &venv)
        .ok_or("the venv folder was not accepted")?;
    assert_eq!(from_folder.path(), Some(venv.join("bin/python").as_path()));
    assert_eq!(from_folder.prefix.as_deref(), Some(venv.as_path()));
    assert_eq!(from_folder.kind, ToolchainKind::new("entered"));
    assert_eq!(from_folder.version, "3.10.14".parse().ok());

    let interpreter = venv.join("bin/python");
    let from_file = entered_toolchain(family, tools.shared_locators(), &request, &interpreter)
        .ok_or("the interpreter was not accepted")?;
    assert_eq!(from_file.path(), Some(interpreter.as_path()));

    assert!(entered_toolchain(
        family,
        tools.shared_locators(),
        &request,
        &root.0.join("empty")
    )
    .is_none());
    Ok(())
}

#[cfg(unix)]
fn fake_binary(path: &Path, version_line: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("#!/bin/sh\necho '{version_line}'\n"))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(unix)]
#[test]
fn every_go_install_hands_its_own_goroot_to_gopls() -> TestResult {
    let root = scratch("goroot")?;
    let home = root.0.join("home");
    let repo = root.0.join("repo");
    std::fs::create_dir_all(&repo)?;
    std::fs::write(repo.join("go.mod"), "module x\n")?;
    let asdf = home.join(".asdf/installs/golang/1.22.4");
    fake_binary(&asdf.join("go/bin/go"), "go version go1.22.4 linux/amd64")?;
    let sdk = home.join("sdk/go1.21.0");
    fake_binary(&sdk.join("bin/go"), "go version go1.21.0 linux/amd64")?;

    let tools = tools(&root.0.join("config"))?;
    let family = tools
        .toolchain_family(&ToolchainFamilyId::new("go"))
        .ok_or("no go family")?;
    let found = discover(
        family,
        tools.shared_locators(),
        &DiscoveryRequest {
            link: &HostLink::Local,
            group_root: &repo,
            project_folder: &ProjectFolder::root(),
            home: Some(&home),
            host_cli: None,
            timeout: TIMEOUT,
        },
    );
    let prefix_of = |kind: &str| {
        found
            .candidates
            .iter()
            .find(|toolchain| toolchain.kind == ToolchainKind::new(kind))
            .and_then(|toolchain| toolchain.prefix.clone())
    };
    assert_eq!(prefix_of("asdf"), Some(asdf.join("go")));
    assert_eq!(prefix_of("sdk"), Some(sdk));
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_entered_windows_style_venv_is_accepted() -> TestResult {
    let root = scratch("scripts-venv")?;
    let repo = root.0.join("repo");
    let venv = repo.join(".venv");
    std::fs::create_dir_all(&venv)?;
    std::fs::write(venv.join("pyvenv.cfg"), "version = 3.12.1\n")?;
    fake_binary(&venv.join("Scripts/python.exe"), "Python 3.12.1")?;
    let tools = tools(&root.0.join("config"))?;
    let family = tools
        .toolchain_family(&ToolchainFamilyId::new("python"))
        .ok_or("no python family")?;
    let request = DiscoveryRequest {
        link: &HostLink::Local,
        group_root: &repo,
        project_folder: &ProjectFolder::root(),
        home: None,
        host_cli: None,
        timeout: TIMEOUT,
    };
    let entered = entered_toolchain(family, tools.shared_locators(), &request, &venv)
        .ok_or("venv not accepted")?;
    assert_eq!(
        entered.location,
        ToolchainLocation::Path {
            path: venv.join("Scripts/python.exe")
        }
    );
    Ok(())
}
