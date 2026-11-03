use krad_domain::{ProjectFolder, ToolchainFamilyId, ToolchainLocation};
use krad_extensions::{discover, DiscoveryRequest, LanguageTools};
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

fn tools(config: &Path) -> Result<LanguageTools, Box<dyn std::error::Error>> {
    Ok(LanguageTools::load(config, &[])?)
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
mod unix;
