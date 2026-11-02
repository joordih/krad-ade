use krad_domain::{ServerLaunch, Toolchain};
use krad_host::HostLink;
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::handoff::{vars, Handoff};
use super::manifest::ToolchainFamilyManifest;
use crate::template::render;

const PATH_VARIABLE: &str = "PATH";
const REMOTE_PATH_SEPARATOR: &str = ":";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathStyle {
    Native,
    Posix,
}

impl From<&HostLink> for PathStyle {
    fn from(link: &HostLink) -> Self {
        if link.is_local() {
            Self::Native
        } else {
            Self::Posix
        }
    }
}

fn joined_path(
    style: PathStyle,
    front: &[PathBuf],
    existing: Option<OsString>,
) -> Option<OsString> {
    if style == PathStyle::Native {
        let inherited = existing.or_else(|| std::env::var_os(PATH_VARIABLE));
        let rest: Vec<PathBuf> = inherited
            .as_deref()
            .map_or_else(Vec::new, |value| std::env::split_paths(value).collect());
        return std::env::join_paths(front.iter().cloned().chain(rest)).ok();
    }
    let mut parts: Vec<String> = front
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    if let Some(existing) = existing.filter(|value| !value.is_empty()) {
        parts.push(existing.to_string_lossy().into_owned());
    }
    Some(OsString::from(parts.join(REMOTE_PATH_SEPARATOR)))
}

#[must_use]
pub fn launch_with(mut launch: ServerLaunch, handoff: &Handoff, link: &HostLink) -> ServerLaunch {
    if !handoff.path.is_empty() {
        let existing = launch
            .env
            .iter()
            .position(|(name, _)| name == PATH_VARIABLE)
            .map(|index| launch.env.remove(index).1);
        if let Some(path) = joined_path(PathStyle::from(link), &handoff.path, existing) {
            launch.env.push((PATH_VARIABLE.to_owned(), path));
        }
    }
    for (name, value) in &handoff.env {
        launch.env.retain(|(known, _)| known != name);
        launch.env.push((name.clone(), OsString::from(value)));
    }
    launch.args.extend(handoff.args.iter().cloned());
    launch
}

fn merge_into(target: &mut Value, overlay: Value) {
    match (target, overlay) {
        (Value::Object(target), Value::Object(overlay)) => {
            for (key, value) in overlay {
                match target.get_mut(&key) {
                    Some(existing) => merge_into(existing, value),
                    None => {
                        target.insert(key, value);
                    }
                }
            }
        }
        (target, overlay) => *target = overlay,
    }
}

#[must_use]
pub fn merged_settings(base: Value, handoff: &Handoff) -> Value {
    let Ok(overlay) = serde_json::to_value(&handoff.settings) else {
        return base;
    };
    if handoff.settings.is_empty() {
        return base;
    }
    let mut merged = if base.is_object() {
        base
    } else {
        Value::Object(serde_json::Map::new())
    };
    merge_into(&mut merged, overlay);
    merged
}

#[must_use]
pub fn activation_lines(family: &ToolchainFamilyManifest, toolchain: &Toolchain) -> Vec<String> {
    let Some(activation) = family.activation(&toolchain.kind) else {
        return Vec::new();
    };
    let vars = vars(toolchain);
    activation
        .script
        .iter()
        .filter_map(|line| match render(line, &vars, Path::new("")) {
            Ok(rendered) => Some(rendered),
            Err(error) => {
                tracing::debug!(%error, "skipped an activation line this toolchain cannot fill");
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolchains::manifest::HandoffApply;
    use krad_domain::{LocatorId, ToolchainFamilyId, ToolchainKind, ToolchainLocation};

    fn launch() -> ServerLaunch {
        ServerLaunch {
            program: PathBuf::from("/srv/.krad-ade/languages/basedpyright/bin/langserver"),
            args: vec!["--stdio".to_owned()],
            env: vec![
                (PATH_VARIABLE.to_owned(), OsString::from("/opt/node/bin")),
                ("VIRTUAL_ENV".to_owned(), OsString::from("/old")),
            ],
        }
    }

    fn handed() -> Handoff {
        let mut settings = toml::Table::new();
        super::super::handoff::insert_dotted(
            &mut settings,
            "python.pythonPath",
            toml::Value::String("/srv/app/.venv/bin/python".to_owned()),
        );
        Handoff {
            settings,
            env: vec![("VIRTUAL_ENV".to_owned(), "/srv/app/.venv".to_owned())],
            path: vec![PathBuf::from("/srv/app/.venv/bin")],
            args: vec!["--verbose".to_owned()],
            apply: HandoffApply::Live,
        }
    }

    fn venv() -> Toolchain {
        Toolchain {
            family: ToolchainFamilyId::new("python"),
            kind: ToolchainKind::new("venv"),
            name: ".venv".to_owned(),
            version: "3.12.4".parse().ok(),
            location: ToolchainLocation::Path {
                path: PathBuf::from("/srv/my app/.venv/bin/python"),
            },
            prefix: Some(PathBuf::from("/srv/my app/.venv")),
            project: None,
            locator: LocatorId::new("pet"),
        }
    }

    fn family() -> Result<ToolchainFamilyManifest, toml::de::Error> {
        toml::from_str(
            r#"
id = "python"
name = "Python"
languages = ["python"]

[[activation]]
kinds = ["venv"]
script = [". {{toolchain.prefix | quote}}/bin/activate"]

[[activation]]
script = ["export PATH={{toolchain.dir | quote}}:\"$PATH\""]
"#,
        )
    }

    #[test]
    fn the_toolchain_goes_first_on_the_path_and_overrides_env() {
        let local = launch_with(launch(), &handed(), &HostLink::Local);
        assert!(local.args.ends_with(&["--verbose".to_owned()]));
        let venv_env = local
            .env
            .iter()
            .filter(|(name, _)| name == "VIRTUAL_ENV")
            .collect::<Vec<_>>();
        assert_eq!(venv_env.len(), 1);
        assert_eq!(venv_env[0].1, OsString::from("/srv/app/.venv"));
        let path = local
            .env
            .iter()
            .find(|(name, _)| name == PATH_VARIABLE)
            .map_or_else(OsString::new, |(_, value)| value.clone());
        let entries: Vec<PathBuf> = std::env::split_paths(&path).collect();
        assert_eq!(
            entries,
            [
                PathBuf::from("/srv/app/.venv/bin"),
                PathBuf::from("/opt/node/bin")
            ]
        );
    }

    #[test]
    fn a_remote_path_joins_with_colons_ahead_of_the_server_runtimes() {
        let front = [PathBuf::from("/srv/app/.venv/bin")];
        assert_eq!(
            joined_path(
                PathStyle::Posix,
                &front,
                Some(OsString::from("/opt/node/bin"))
            ),
            Some(OsString::from("/srv/app/.venv/bin:/opt/node/bin"))
        );
        assert_eq!(
            joined_path(PathStyle::Posix, &front, None),
            Some(OsString::from("/srv/app/.venv/bin"))
        );
    }

    #[test]
    fn manifest_settings_are_kept_under_the_toolchains_keys() {
        let base = serde_json::json!({
            "python": { "analysis": { "autoSearchPaths": true } },
            "rust-analyzer": { "check": { "command": "clippy" } }
        });
        let merged = merged_settings(base, &handed());
        assert_eq!(
            merged["python"]["pythonPath"],
            Value::from("/srv/app/.venv/bin/python")
        );
        assert_eq!(
            merged["python"]["analysis"]["autoSearchPaths"],
            Value::Bool(true)
        );
        assert_eq!(
            merged["rust-analyzer"]["check"]["command"],
            Value::from("clippy")
        );
        let fresh = merged_settings(Value::Null, &handed());
        assert_eq!(
            fresh["python"]["pythonPath"],
            Value::from("/srv/app/.venv/bin/python")
        );
    }

    #[test]
    fn activation_picks_the_kinds_script_and_quotes_paths() -> Result<(), toml::de::Error> {
        let family = family()?;
        assert_eq!(
            activation_lines(&family, &venv()),
            [". '/srv/my app/.venv'/bin/activate"]
        );
        let system = Toolchain {
            kind: ToolchainKind::new("system"),
            location: ToolchainLocation::Path {
                path: PathBuf::from("/usr/bin/python3"),
            },
            prefix: None,
            ..venv()
        };
        assert_eq!(
            activation_lines(&family, &system),
            ["export PATH='/usr/bin':\"$PATH\""]
        );
        let bundled = Toolchain {
            location: ToolchainLocation::Bundled,
            prefix: None,
            ..system
        };
        assert_eq!(activation_lines(&family, &bundled), Vec::<String>::new());
        Ok(())
    }
}
