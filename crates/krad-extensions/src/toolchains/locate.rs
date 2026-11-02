use krad_domain::{
    LocatorId, Toolchain, ToolchainFamilyId, ToolchainKind, ToolchainLocation, ToolchainVersion,
};
use krad_host::{HostCommand, HostLink};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::glob::expand;
use super::manifest::{
    BundledLocator, CommandLocator, EnvLocator, GlobBase, GlobLocator, LocatorSpec, SharedLocator,
    SharedLocatorManifest, VersionSource,
};
use super::output::{parse_output, version_after, Found};
use super::pet::run_pet;
use crate::template::{render, Vars};

const MATCH_VAR: &str = "match";
const BINARY_FOLDERS: [&str; 2] = ["bin", "Scripts"];
const MATCH_NAME_VAR: &str = "match_name";
const MATCH_DIR_VAR: &str = "match_dir";
const HOME_VAR: &str = "home";
const GROUP_ROOT_VAR: &str = "group_root";
const PROJECT_FOLDER_VAR: &str = "project_folder";
const VERSION_VAR: &str = "version";
const NAME_VAR: &str = "name";
const PRINTENV_PROGRAM: &str = "printenv";
const VERSION_FILE_LIMIT: u64 = 64 * 1024;
const ROOT: &str = "/";
const ENTERED_VAR: &str = "entered";

pub(crate) struct LocateContext<'a> {
    pub(crate) link: &'a HostLink,
    pub(crate) family: &'a ToolchainFamilyId,
    pub(crate) home: Option<&'a Path>,
    pub(crate) group_root: &'a Path,
    pub(crate) project_folder: &'a Path,
    pub(crate) timeout: Duration,
    pub(crate) host_cli: Option<&'a Path>,
    pub(crate) shared: &'a [SharedLocatorManifest],
    pub(crate) entered: Option<&'a Path>,
}

impl LocateContext<'_> {
    fn vars(&self, params: &BTreeMap<String, String>) -> Vars {
        let mut vars = Vars::default()
            .with(GROUP_ROOT_VAR, self.group_root.to_string_lossy())
            .with(PROJECT_FOLDER_VAR, self.project_folder.to_string_lossy());
        if let Some(home) = self.home {
            vars.set(HOME_VAR, home.to_string_lossy());
        }
        if let Some(entered) = self.entered {
            vars.set(ENTERED_VAR, entered.to_string_lossy());
        }
        for (key, value) in params {
            vars.set(key.clone(), value.clone());
        }
        vars
    }

    fn run(&self, command: &HostCommand) -> Option<String> {
        let output = self.link.output_within(command, self.timeout).ok()?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        if !output.status.success() {
            tracing::debug!(
                program = %command.program_name(),
                status = ?output.status.code(),
                "a toolchain locator command ended with a failure status"
            );
        }
        (!text.trim().is_empty()).then_some(text)
    }

    fn base(&self, base: GlobBase) -> Option<&Path> {
        match base {
            GlobBase::ProjectFolder => Some(self.project_folder),
            GlobBase::GroupRoot => Some(self.group_root),
            GlobBase::Home => self.home,
            GlobBase::Absolute => Some(Path::new(ROOT)),
            GlobBase::Entered => self.entered,
        }
    }
}

fn filled(template: Option<&String>, vars: &Vars, dir: &Path) -> Option<String> {
    template.and_then(|text| render(text, vars, dir).ok())
}

fn with_match(vars: &Vars, value: &str) -> Vars {
    let path = Path::new(value);
    let mut vars = vars.clone().with(MATCH_VAR, value);
    if let Some(name) = path.file_name() {
        vars.set(MATCH_NAME_VAR, name.to_string_lossy());
    }
    if let Some(dir) = path.parent() {
        vars.set(MATCH_DIR_VAR, dir.to_string_lossy());
    }
    vars
}

struct Shape<'a> {
    id: &'a LocatorId,
    kind: &'a ToolchainKind,
    path: Option<&'a String>,
    prefix: Option<&'a String>,
    name: Option<&'a String>,
    version: Option<&'a VersionSource>,
}

fn probe_version(
    context: &LocateContext<'_>,
    source: Option<&VersionSource>,
    vars: &Vars,
    program: &Path,
    found: Option<&str>,
) -> Option<ToolchainVersion> {
    if let Some(version) = found.and_then(|text| text.parse().ok()) {
        return Some(version);
    }
    match source? {
        VersionSource::Command {
            program: probe,
            args,
            after,
        } => {
            let probe = filled(probe.as_ref(), vars, program)
                .map_or_else(|| program.to_path_buf(), PathBuf::from);
            let mut command = HostCommand::new(&probe);
            command
                .args(
                    args.iter()
                        .filter_map(|arg| render(arg, vars, program).ok()),
                )
                .in_login_shell();
            version_after(&context.run(&command)?, after.as_deref())
        }
        VersionSource::JsonFile { file, pointer } => {
            let file = PathBuf::from(render(file, vars, program).ok()?);
            let bytes = context.link.read(&file, VERSION_FILE_LIMIT).ok()?;
            let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            json.pointer(pointer)?.as_str()?.parse().ok()
        }
        VersionSource::TextFile { file } => {
            let file = PathBuf::from(render(file, vars, program).ok()?);
            let bytes = context.link.read(&file, VERSION_FILE_LIMIT).ok()?;
            version_after(&String::from_utf8_lossy(&bytes), None)
        }
        VersionSource::Name => vars
            .get(MATCH_NAME_VAR)
            .and_then(|name| version_after(name, None)),
    }
}

fn shaped(
    context: &LocateContext<'_>,
    shape: &Shape<'_>,
    vars: &Vars,
    found: &Found,
) -> Option<Toolchain> {
    let mut vars = with_match(vars, &found.value);
    if let Some(version) = &found.version {
        vars.set(VERSION_VAR, version.clone());
    }
    if let Some(name) = &found.name {
        vars.set(NAME_VAR, name.clone());
    }
    let dir = Path::new(&found.value);
    let path = filled(shape.path, &vars, dir).unwrap_or_else(|| found.value.clone());
    let path = PathBuf::from(path);
    if !context.link.exists(&path) {
        return None;
    }
    let version = probe_version(
        context,
        shape.version,
        &vars,
        &path,
        found.version.as_deref(),
    );
    let name = filled(shape.name, &vars, dir)
        .or_else(|| found.name.clone())
        .or_else(|| vars.get(MATCH_NAME_VAR).map(str::to_owned))
        .unwrap_or_else(|| found.value.clone());
    Some(Toolchain {
        family: context.family.clone(),
        kind: shape.kind.clone(),
        name,
        version,
        prefix: filled(shape.prefix, &vars, dir)
            .map(PathBuf::from)
            .or_else(|| prefix_of_binary(&path)),
        location: ToolchainLocation::Path { path },
        project: None,
        locator: shape.id.clone(),
    })
}

pub(crate) fn prefix_of_binary(binary: &Path) -> Option<PathBuf> {
    let folder = binary.parent()?;
    let named = folder.file_name()?.to_str()?;
    BINARY_FOLDERS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(named))
        .then(|| folder.parent().map(Path::to_owned))
        .flatten()
}

fn require_met(context: &LocateContext<'_>, matched: &Path, require: &[String]) -> bool {
    require
        .iter()
        .all(|file| context.link.exists(&matched.join(file)))
}

fn glob(
    context: &LocateContext<'_>,
    locator: &GlobLocator,
    id: &LocatorId,
    vars: &Vars,
) -> Vec<Toolchain> {
    let Some(base) = context.base(locator.base) else {
        return Vec::new();
    };
    let shape = Shape {
        id,
        kind: &locator.kind,
        path: locator.path.as_ref(),
        prefix: locator.prefix.as_ref(),
        name: locator.name.as_ref(),
        version: locator.version.as_ref(),
    };
    locator
        .patterns
        .iter()
        .filter_map(|pattern| render(pattern, vars, base).ok())
        .flat_map(|pattern| expand(context.link, base, &pattern))
        .filter(|matched| require_met(context, matched, &locator.require))
        .filter_map(|matched| {
            let found = Found {
                value: matched.to_string_lossy().into_owned(),
                version: None,
                name: None,
            };
            shaped(context, &shape, vars, &found)
        })
        .collect()
}

fn command(
    context: &LocateContext<'_>,
    locator: &CommandLocator,
    id: &LocatorId,
    vars: &Vars,
) -> Vec<Toolchain> {
    let dir = context.project_folder;
    let mut run = HostCommand::new(&locator.program);
    run.args(
        locator
            .args
            .iter()
            .filter_map(|arg| render(arg, vars, dir).ok()),
    )
    .current_dir(dir)
    .in_login_shell();
    let Some(text) = context.run(&run) else {
        return Vec::new();
    };
    let shape = Shape {
        id,
        kind: &locator.kind,
        path: locator.path.as_ref(),
        prefix: locator.prefix.as_ref(),
        name: locator.name.as_ref(),
        version: locator.version.as_ref(),
    };
    parse_output(&text, &locator.output)
        .iter()
        .filter_map(|found| shaped(context, &shape, vars, found))
        .collect()
}

fn env_value(context: &LocateContext<'_>, var: &str) -> Option<String> {
    if context.link.is_local() {
        return std::env::var(var).ok().filter(|value| !value.is_empty());
    }
    let mut printenv = HostCommand::new(PRINTENV_PROGRAM);
    printenv.arg(var).in_login_shell();
    context
        .run(&printenv)
        .map(|text| text.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn env(context: &LocateContext<'_>, locator: &EnvLocator, vars: &Vars) -> Vec<Toolchain> {
    let Some(value) = env_value(context, &locator.var) else {
        return Vec::new();
    };
    let shape = Shape {
        id: &locator.id,
        kind: &locator.kind,
        path: locator.path.as_ref(),
        prefix: None,
        name: locator.name.as_ref(),
        version: locator.version.as_ref(),
    };
    let found = Found {
        value,
        version: None,
        name: None,
    };
    shaped(context, &shape, vars, &found).into_iter().collect()
}

fn bundled(context: &LocateContext<'_>, locator: &BundledLocator) -> Toolchain {
    Toolchain {
        family: context.family.clone(),
        kind: locator.kind.clone(),
        name: locator.name.clone(),
        version: None,
        location: ToolchainLocation::Bundled,
        prefix: None,
        project: None,
        locator: locator.id.clone(),
    }
}

fn shared(context: &LocateContext<'_>, locator: &SharedLocator) -> Vec<Toolchain> {
    let Some(definition) = context
        .shared
        .iter()
        .find(|known| *known.id() == locator.uses)
    else {
        tracing::warn!(locator = %locator.uses, "a toolchain manifest uses an unknown shared locator");
        return Vec::new();
    };
    let vars = context.vars(&locator.params);
    match definition {
        SharedLocatorManifest::Glob(body) => glob(context, body, &locator.id, &vars),
        SharedLocatorManifest::Command(body) => command(context, body, &locator.id, &vars),
    }
}

pub(crate) fn locate_entered(
    context: &LocateContext<'_>,
    locators: &[GlobLocator],
) -> Option<Toolchain> {
    let vars = context.vars(&BTreeMap::new());
    locators.iter().find_map(|locator| {
        glob(context, locator, &locator.id, &vars)
            .into_iter()
            .find(|found| found.version.is_some())
    })
}

pub(crate) fn locate(context: &LocateContext<'_>, spec: &LocatorSpec) -> Vec<Toolchain> {
    let vars = context.vars(&BTreeMap::new());
    match spec {
        LocatorSpec::Pet { id } => run_pet(context, id),
        LocatorSpec::Glob(locator) => glob(context, locator, &locator.id, &vars),
        LocatorSpec::Command(locator) => command(context, locator, &locator.id, &vars),
        LocatorSpec::Env(locator) => env(context, locator, &vars),
        LocatorSpec::Bundled(locator) => vec![bundled(context, locator)],
        LocatorSpec::Shared(locator) => shared(context, locator),
    }
}

impl LocateContext<'_> {
    pub(crate) fn output_text(&self, command: &HostCommand) -> Option<String> {
        let output = self.link.output_within(command, self.timeout).ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binary_in_bin_or_scripts_names_its_install_folder() {
        assert_eq!(
            prefix_of_binary(Path::new("/opt/jdk-21/bin/java")),
            Some(PathBuf::from("/opt/jdk-21"))
        );
        assert_eq!(
            prefix_of_binary(Path::new("/w/.venv/Scripts/python.exe")),
            Some(PathBuf::from("/w/.venv"))
        );
        assert_eq!(prefix_of_binary(Path::new("/usr/local/go-wrapper")), None);
    }
}
