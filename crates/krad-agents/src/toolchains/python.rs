use krad_domain::{LocatorId, Toolchain, ToolchainFamilyId, ToolchainKind, ToolchainLocation};
use pet::find::find_and_report_envs;
use pet::locators::create_locators;
use pet_conda::Conda;
use pet_core::os_environment::EnvironmentApi;
use pet_core::python_environment::{PythonEnvironment, PythonEnvironmentKind};
use pet_core::Configuration;
use pet_poetry::Poetry;
use pet_reporter::collect::CollectReporter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError};

const FAMILY: &str = "python";
const LOCATOR: &str = "pet";

const fn kind_id(kind: PythonEnvironmentKind) -> &'static str {
    match kind {
        PythonEnvironmentKind::Conda => "conda",
        PythonEnvironmentKind::Pixi => "pixi",
        PythonEnvironmentKind::Homebrew => "homebrew",
        PythonEnvironmentKind::Pyenv => "pyenv",
        PythonEnvironmentKind::GlobalPaths => "global-paths",
        PythonEnvironmentKind::PyenvVirtualEnv => "pyenv-virtualenv",
        PythonEnvironmentKind::Pipenv => "pipenv",
        PythonEnvironmentKind::Poetry => "poetry",
        PythonEnvironmentKind::Hatch => "hatch",
        PythonEnvironmentKind::MacPythonOrg => "mac-python-org",
        PythonEnvironmentKind::MacCommandLineTools => "mac-command-line-tools",
        PythonEnvironmentKind::LinuxGlobal => "linux-global",
        PythonEnvironmentKind::MacXCode => "mac-xcode",
        PythonEnvironmentKind::Uv => "uv",
        PythonEnvironmentKind::UvWorkspace => "uv-workspace",
        PythonEnvironmentKind::Venv => "venv",
        PythonEnvironmentKind::VirtualEnv => "virtualenv",
        PythonEnvironmentKind::VirtualEnvWrapper => "virtualenvwrapper",
        PythonEnvironmentKind::WinPython => "winpython",
        PythonEnvironmentKind::WindowsStore => "windows-store",
        PythonEnvironmentKind::WindowsRegistry => "windows-registry",
    }
}

fn name_of(environment: &PythonEnvironment, executable: &Path) -> String {
    environment
        .display_name
        .clone()
        .or_else(|| environment.name.clone())
        .or_else(|| {
            environment
                .prefix
                .as_deref()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| executable.to_string_lossy().into_owned())
}

fn toolchain(environment: &PythonEnvironment) -> Option<Toolchain> {
    if environment.error.is_some() {
        return None;
    }
    let executable = environment.executable.clone()?;
    let kind = environment.kind.map_or("system", kind_id);
    Some(Toolchain {
        family: ToolchainFamilyId::new(FAMILY),
        kind: ToolchainKind::new(kind),
        name: name_of(environment, &executable),
        version: environment
            .version
            .as_deref()
            .and_then(|version| version.parse().ok()),
        location: ToolchainLocation::Path { path: executable },
        prefix: environment.prefix.clone(),
        project: environment.project.clone(),
        locator: LocatorId::new(LOCATOR),
    })
}

pub(super) fn environments(root: &Path, folder: Option<&Path>) -> Vec<Toolchain> {
    let environment = EnvironmentApi::new();
    let conda = Arc::new(Conda::from(&environment));
    let poetry = Arc::new(Poetry::from(&environment));
    let locators = create_locators(conda, poetry, &environment);
    let mut workspaces: Vec<PathBuf> = vec![root.to_path_buf()];
    workspaces.extend(
        folder
            .filter(|folder| *folder != root)
            .map(Path::to_path_buf),
    );
    let configuration = Configuration {
        workspace_directories: Some(workspaces),
        ..Configuration::default()
    };
    for locator in locators.iter() {
        locator.configure(&configuration);
    }
    let reporter = CollectReporter::new();
    find_and_report_envs(
        &reporter,
        configuration,
        &locators,
        &environment,
        None,
        None,
    );
    let found = reporter
        .environments
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut unique: Vec<Toolchain> = Vec::with_capacity(found.len());
    for candidate in found.iter().filter_map(toolchain) {
        if !unique.iter().any(|known| known.is_same(&candidate)) {
            unique.push(candidate);
        }
    }
    unique
}
