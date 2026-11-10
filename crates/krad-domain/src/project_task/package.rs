use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{ProjectTask, TaskSource};

const RUN_COMMAND: &str = "run";
const OPTION_PREFIX: char = '-';
const SCRIPT_PUNCTUATION: [char; 7] = ['-', '_', ':', '.', '/', '@', '+'];

fn is_script_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(OPTION_PREFIX)
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || SCRIPT_PUNCTUATION.contains(&ch))
}

pub const PACKAGE_LOCKFILES: [(&str, NodePackageManager); 4] = [
    ("pnpm-lock.yaml", NodePackageManager::Pnpm),
    ("yarn.lock", NodePackageManager::Yarn),
    ("bun.lock", NodePackageManager::Bun),
    ("bun.lockb", NodePackageManager::Bun),
];

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum NodePackageManager {
    #[default]
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl NodePackageManager {
    #[must_use]
    pub const fn program(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    #[must_use]
    pub fn from_lockfiles(present: impl Fn(&str) -> bool) -> Self {
        PACKAGE_LOCKFILES
            .iter()
            .find(|(file, _)| present(file))
            .map_or_else(Self::default, |(_, manager)| *manager)
    }
}

#[derive(Deserialize)]
struct PackageManifest {
    #[serde(default)]
    scripts: BTreeMap<String, String>,
}

/// # Errors
///
/// Returns the JSON error when the manifest is not a valid `package.json`.
pub fn package_script_tasks(
    manifest: &str,
    manager: NodePackageManager,
) -> Result<Vec<ProjectTask>, serde_json::Error> {
    let manifest: PackageManifest = serde_json::from_str(manifest)?;
    let program = manager.program();
    Ok(manifest
        .scripts
        .keys()
        .filter(|name| is_script_name(name))
        .map(|name| {
            ProjectTask::new(
                TaskSource::PackageScripts(manager),
                name,
                program,
                &[RUN_COMMAND, name],
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_that_read_as_options_or_carry_unsafe_characters_are_left_out(
    ) -> Result<(), serde_json::Error> {
        let manifest = r#"{"scripts": {
            "test:unit": "x", "build.prod": "x", "@scope/lint": "x",
            "--inspect": "x", "-x": "x", "a b": "x", "rm;ls": "x", "$(id)": "x", "": "x"
        }}"#;
        let tasks = package_script_tasks(manifest, NodePackageManager::Npm)?;
        let names: Vec<&str> = tasks.iter().map(|task| task.name.as_str()).collect();
        assert_eq!(names, ["@scope/lint", "build.prod", "test:unit"]);
        Ok(())
    }

    #[test]
    fn each_script_runs_through_the_package_manager() -> Result<(), serde_json::Error> {
        let manifest = r#"{"name":"web","scripts":{"dev":"vite","build":"vite build"}}"#;
        let tasks = package_script_tasks(manifest, NodePackageManager::Pnpm)?;
        let lines: Vec<String> = tasks.iter().map(ToString::to_string).collect();
        assert_eq!(lines, ["pnpm run build", "pnpm run dev"]);
        assert_eq!(tasks[0].name, "build");
        assert_eq!(package_script_tasks("{}", NodePackageManager::Npm)?, []);
        assert!(package_script_tasks("{", NodePackageManager::Npm).is_err());
        Ok(())
    }

    #[test]
    fn the_lockfile_picks_the_package_manager() {
        assert_eq!(
            NodePackageManager::from_lockfiles(|file| file == "yarn.lock"),
            NodePackageManager::Yarn
        );
        assert_eq!(
            NodePackageManager::from_lockfiles(|file| file == "bun.lockb"),
            NodePackageManager::Bun
        );
        assert_eq!(
            NodePackageManager::from_lockfiles(|_| false),
            NodePackageManager::Npm
        );
    }
}
