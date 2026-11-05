mod contributions;
mod program;
mod values;

pub use contributions::{
    CommandContribution, Contributions, HostKindContribution, KeybindingContribution,
    MenuContribution, MenuTarget, SecretContribution, SettingContribution, SettingKind,
    SettingOption, ToolContribution, ToolEffect, ViewContribution, ViewZone,
};
pub use program::{Activation, Binary, BinarySource, Launch, Permission, Placement, Program};
pub use values::{HttpsUrl, ProgramPath, Sha256Hex, ValueError};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

use crate::id::{CommandId, ExtensionId, ViewId};
use crate::version::ApiVersion;

pub const MANIFEST_FILE: &str = "extension.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    #[schemars(with = "String")]
    pub id: ExtensionId,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    pub api: ApiVersion,
    #[serde(default)]
    pub program: Option<Program>,
    #[serde(default)]
    pub activation: Vec<Activation>,
    #[serde(default)]
    pub contributes: Contributions,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManifestError {
    #[error("{0} is declared twice")]
    Duplicate(String),
    #[error("{0} names a command the extension does not declare")]
    UnknownCommand(CommandId),
    #[error("the extension declares commands, views, tools or host kinds but brings no program")]
    NoProgram,
}

impl ExtensionManifest {
    /// # Errors
    ///
    /// Returns [`ManifestError`] when two contributions share an id, a menu or keybinding names an
    /// undeclared command, or commands and views are declared without a program to answer them.
    pub fn check(&self) -> Result<(), ManifestError> {
        let commands = unique(self.contributes.commands.iter().map(|command| &command.id))?;
        unique::<ViewId>(self.contributes.views.iter().map(|view| &view.id))?;
        unique(self.contributes.settings.iter().map(|setting| &setting.key))?;
        unique(self.contributes.secrets.iter().map(|secret| &secret.name))?;
        unique(self.contributes.tools.iter().map(|tool| &tool.name))?;
        unique(self.contributes.host_kinds.iter().map(|kind| &kind.id))?;
        let referenced = self
            .contributes
            .menus
            .iter()
            .map(|menu| &menu.command)
            .chain(
                self.contributes
                    .keybindings
                    .iter()
                    .map(|binding| &binding.command),
            );
        if let Some(missing) = referenced
            .into_iter()
            .find(|command| !commands.contains(command))
        {
            return Err(ManifestError::UnknownCommand(missing.clone()));
        }
        let answers_something = !self.contributes.commands.is_empty()
            || !self.contributes.views.is_empty()
            || !self.contributes.tools.is_empty()
            || !self.contributes.host_kinds.is_empty();
        if answers_something && self.program.is_none() {
            return Err(ManifestError::NoProgram);
        }
        Ok(())
    }
}

fn unique<'a, T>(ids: impl Iterator<Item = &'a T>) -> Result<BTreeSet<&'a T>, ManifestError>
where
    T: Ord + std::fmt::Display + 'a,
{
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(ManifestError::Duplicate(id.to_string()));
        }
    }
    Ok(seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const DOCKER: &str = r#"
id = "docker"
name = "Docker"
version = "0.1.0"
api = "1.0"
activation = ["startup", { workspace_contains = "docker-compose.yml" }]

[program]
placement = "app"
runtime = "node"
args = ["dist/main.js"]

[[contributes.command]]
id = "containers.refresh"
title = "Docker: Refresh containers"
icon = "refresh"

[[contributes.menu]]
command = "containers.refresh"
target = "file"
files = ["Dockerfile", "docker-compose.yml"]

[[contributes.keybinding]]
command = "containers.refresh"
macos = "cmd-alt-r"

[[contributes.view]]
id = "containers"
title = "Containers"
zone = "sidebar"
icon = "layers"

[[contributes.setting]]
key = "socket"
title = "Docker socket"
type = "text"
default = "/var/run/docker.sock"

[[contributes.setting]]
key = "show-stopped"
title = "Show stopped containers"
type = "toggle"
default = true

[[contributes.secret]]
name = "registry-token"
title = "Registry token"
"#;

    fn docker() -> Result<ExtensionManifest, toml::de::Error> {
        toml::from_str(DOCKER)
    }

    #[test]
    fn a_docker_manifest_reads_with_every_contribution() -> TestResult {
        let manifest = docker()?;
        assert_eq!(manifest.api, ApiVersion::new(1, 0));
        assert_eq!(
            manifest.program.as_ref().map(|program| &program.launch),
            Some(&Launch::Runtime {
                runtime: "node".parse()?,
                args: vec!["dist/main.js".to_owned()],
            })
        );
        assert_eq!(
            manifest.activation,
            [
                Activation::Startup,
                Activation::WorkspaceContains("docker-compose.yml".to_owned())
            ]
        );
        assert_eq!(manifest.contributes.views[0].zone, ViewZone::Sidebar);
        assert_eq!(
            manifest.contributes.settings[1].kind,
            SettingKind::Toggle { default: true }
        );
        assert_eq!(manifest.check(), Ok(()));
        Ok(())
    }

    #[test]
    fn a_binary_program_names_its_files_per_platform() -> TestResult {
        let text = r#"
id = "tool"
name = "Tool"
version = "1.0.0"
api = "1.0"

[program]
placement = "environment"

[[program.binary]]
targets = ["darwin_arm64"]
path = "bin/tool-macos"

[[program.binary]]
targets = ["linux_x64"]
url = "https://example.com/tool-linux"
sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
"#;
        let manifest: ExtensionManifest = toml::from_str(text)?;
        let program = manifest.program.ok_or("the manifest brings a program")?;
        assert_eq!(program.placement, Placement::Environment);
        let binary = match program.launch {
            Launch::Binary { binary, .. } => binary,
            Launch::Runtime { .. } => Vec::new(),
        };
        assert_eq!(
            binary.first().map(|binary| &binary.source),
            Some(&BinarySource::Bundled {
                path: "bin/tool-macos".parse()?
            })
        );
        assert!(matches!(
            binary.get(1).map(|binary| &binary.source),
            Some(BinarySource::Download { .. })
        ));
        Ok(())
    }

    #[test]
    fn binaries_that_leave_the_folder_or_skip_their_checksum_are_refused() {
        let binary = |source: &str| {
            format!(
                "id = \"tool\"\nname = \"Tool\"\nversion = \"1\"\napi = \"1.0\"\n\n[program]\nplacement = \"app\"\n\n[[program.binary]]\ntargets = [\"linux_x64\"]\n{source}\n"
            )
        };
        for bad in [
            "path = \"/usr/bin/env\"",
            "path = \"../../bin/sh\"",
            "url = \"https://example.com/x\"\nsha256 = \"../../../../tmp/evil\"",
            "url = \"http://example.com/x\"\nsha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"",
        ] {
            assert!(
                toml::from_str::<ExtensionManifest>(&binary(bad)).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn unknown_fields_and_icons_are_refused() {
        let typo = DOCKER.replace("activation =", "activations =");
        assert!(toml::from_str::<ExtensionManifest>(&typo).is_err());
        let icon = DOCKER.replace(r#"icon = "layers""#, r#"icon = "rocket""#);
        assert!(toml::from_str::<ExtensionManifest>(&icon).is_err());
    }

    #[test]
    fn a_menu_for_an_undeclared_command_fails_the_check() -> TestResult {
        let mut manifest = docker()?;
        let missing: CommandId = "missing".parse()?;
        manifest.contributes.menus[0].command = missing.clone();
        assert_eq!(
            manifest.check(),
            Err(ManifestError::UnknownCommand(missing))
        );
        Ok(())
    }

    #[test]
    fn commands_without_a_program_fail_the_check() -> TestResult {
        let mut manifest = docker()?;
        manifest.program = None;
        assert_eq!(manifest.check(), Err(ManifestError::NoProgram));
        Ok(())
    }

    #[test]
    fn duplicated_views_fail_the_check() -> TestResult {
        let mut manifest = docker()?;
        let view = manifest.contributes.views[0].clone();
        manifest.contributes.views.push(view);
        assert_eq!(
            manifest.check(),
            Err(ManifestError::Duplicate("containers".to_owned()))
        );
        Ok(())
    }
}
