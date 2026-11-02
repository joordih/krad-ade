use krad_domain::Toolchain;
use std::path::{Path, PathBuf};

use super::manifest::{HandoffApply, ProjectConfig, ServerToolchain};
use crate::template::{render, Vars};

const PATH_VAR: &str = "toolchain.path";
const DIR_VAR: &str = "toolchain.dir";
const PREFIX_VAR: &str = "toolchain.prefix";
const VERSION_VAR: &str = "toolchain.version";
const NAME_VAR: &str = "toolchain.name";
const KIND_VAR: &str = "toolchain.kind";
const KEY_SEPARATOR: char = '.';

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Handoff {
    pub settings: toml::Table,
    pub env: Vec<(String, String)>,
    pub path: Vec<PathBuf>,
    pub args: Vec<String>,
    pub apply: HandoffApply,
}

pub(crate) fn vars(toolchain: &Toolchain) -> Vars {
    let mut vars = Vars::default()
        .with(NAME_VAR, toolchain.name.clone())
        .with(KIND_VAR, toolchain.kind.to_string());
    if let Some(path) = toolchain.path() {
        vars.set(PATH_VAR, path.to_string_lossy());
        if let Some(dir) = path.parent() {
            vars.set(DIR_VAR, dir.to_string_lossy());
        }
    }
    if let Some(prefix) = &toolchain.prefix {
        vars.set(PREFIX_VAR, prefix.to_string_lossy());
    }
    if let Some(version) = &toolchain.version {
        vars.set(VERSION_VAR, version.to_string());
    }
    vars
}

pub(crate) fn insert_dotted(table: &mut toml::Table, key: &str, value: toml::Value) {
    let mut parts: Vec<&str> = key.split(KEY_SEPARATOR).collect();
    let Some(last) = parts.pop() else {
        return;
    };
    let mut current = table;
    for part in parts {
        let entry = current
            .entry(part.to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if !entry.is_table() {
            *entry = toml::Value::Table(toml::Table::new());
        }
        let toml::Value::Table(next) = entry else {
            return;
        };
        current = next;
    }
    current.insert(last.to_owned(), value);
}

#[must_use]
pub fn handoff(
    server: &ServerToolchain,
    toolchain: Option<&Toolchain>,
    project_config_present: impl Fn(&ProjectConfig) -> bool,
) -> Handoff {
    let vars = toolchain.map_or_else(Vars::default, vars);
    let dir = Path::new("");
    let fill = |template: &String| render(template, &vars, dir).ok();
    let mut settings = toml::Table::new();
    for default in &server.defaults {
        if !default
            .unless_project_config
            .iter()
            .any(&project_config_present)
        {
            insert_dotted(&mut settings, &default.key, default.value.clone());
        }
    }
    for (key, template) in &server.settings {
        if let Some(value) = fill(template) {
            insert_dotted(&mut settings, key, toml::Value::String(value));
        }
    }
    Handoff {
        settings,
        env: server
            .env
            .iter()
            .filter_map(|(key, template)| Some((key.clone(), fill(template)?)))
            .collect(),
        path: server
            .path
            .iter()
            .filter_map(fill)
            .map(PathBuf::from)
            .collect(),
        args: server.args.iter().filter_map(fill).collect(),
        apply: server.apply,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolchains::manifest::ServerDefault;
    use krad_domain::{LocatorId, ToolchainFamilyId, ToolchainKind, ToolchainLocation};
    use std::collections::BTreeMap;

    fn venv() -> Toolchain {
        Toolchain {
            family: ToolchainFamilyId::new("python"),
            kind: ToolchainKind::new("venv"),
            name: ".venv".to_owned(),
            version: "3.12.4".parse().ok(),
            location: ToolchainLocation::Path {
                path: PathBuf::from("/srv/app/.venv/bin/python"),
            },
            prefix: Some(PathBuf::from("/srv/app/.venv")),
            project: None,
            locator: LocatorId::new("pet"),
        }
    }

    fn basedpyright() -> ServerToolchain {
        ServerToolchain {
            family: ToolchainFamilyId::new("python"),
            apply: HandoffApply::Live,
            settings: BTreeMap::from([(
                "python.pythonPath".to_owned(),
                "{{toolchain.path}}".to_owned(),
            )]),
            env: BTreeMap::from([("VIRTUAL_ENV".to_owned(), "{{toolchain.prefix}}".to_owned())]),
            path: vec!["{{toolchain.dir}}".to_owned()],
            args: Vec::new(),
            defaults: vec![ServerDefault {
                key: "basedpyright.analysis.typeCheckingMode".to_owned(),
                value: toml::Value::String("standard".to_owned()),
                unless_project_config: vec![ProjectConfig {
                    file: "pyrightconfig.json".to_owned(),
                    table: None,
                }],
            }],
        }
    }

    #[test]
    fn the_interpreter_and_defaults_reach_the_server_settings() {
        let handed = handoff(&basedpyright(), Some(&venv()), |_| false);
        assert_eq!(
            handed.settings["python"]["pythonPath"].as_str(),
            Some("/srv/app/.venv/bin/python")
        );
        assert_eq!(
            handed.settings["basedpyright"]["analysis"]["typeCheckingMode"].as_str(),
            Some("standard")
        );
        assert_eq!(
            handed.env,
            [("VIRTUAL_ENV".to_owned(), "/srv/app/.venv".to_owned())]
        );
        assert_eq!(handed.path, [PathBuf::from("/srv/app/.venv/bin")]);
    }

    #[test]
    fn project_configuration_keeps_its_own_check_level() {
        let handed = handoff(&basedpyright(), Some(&venv()), |config| {
            config.file == "pyrightconfig.json"
        });
        assert!(handed.settings.get("basedpyright").is_none());
    }

    #[test]
    fn a_bundled_toolchain_sends_no_path() {
        let bundled = Toolchain {
            location: ToolchainLocation::Bundled,
            prefix: None,
            ..venv()
        };
        let handed = handoff(&basedpyright(), Some(&bundled), |_| false);
        assert!(handed.settings.get("python").is_none());
        assert_eq!(handed.env, Vec::new());
        assert_eq!(handed.path, Vec::<PathBuf>::new());
    }

    #[test]
    fn without_a_toolchain_only_the_defaults_reach_the_server() {
        let handed = handoff(&basedpyright(), None, |_| false);
        assert!(handed.settings.get("python").is_none());
        assert_eq!(
            handed.settings["basedpyright"]["analysis"]["typeCheckingMode"].as_str(),
            Some("standard")
        );
        assert!(handed.env.is_empty() && handed.path.is_empty());
    }
}
