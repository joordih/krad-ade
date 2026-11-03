use gpui_kit::Context;
use krad_domain::{
    EffectiveToolchain, EnvironmentName, GroupActivation, GroupId, Host, ProjectFolder,
    TerminalGroup, TerminalId, ToolchainActivation, ToolchainChoices,
};
use krad_extensions::{
    activation_lines, SharedLocatorManifest, ToolchainCache, ToolchainFamilyManifest,
};
use krad_host::{HostCommand, HostLink};
use krad_session::{FirstInput, PtyInput};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};

use super::super::context_menu::{MenuAction, MenuEntry};
use super::super::shell_site::ShellSite;
use super::super::EnvironmentShell;
use super::resolve::{discover_scope, DiscoveryInputs};
use crate::components::IconName;

const POSIX_SHELLS: [&str; 8] = ["sh", "bash", "zsh", "dash", "ksh", "mksh", "ash", "yash"];
const SHELL_VAR: &str = "SHELL";
const PRINTENV_PROGRAM: &str = "printenv";
const SHELL_SUBMIT: &str = "\r";
const THREAD_ACTIVATION: &str = "krad-ade-toolchain-activation";
const ACTIVATION_MENU_LABEL: &str = "Activate toolchain in new terminals";
const INHERIT_LABEL: &str = "Like Settings";
const ON_LABEL: &str = "On";
const OFF_LABEL: &str = "Off";

fn activation_label(activation: GroupActivation, global: ToolchainActivation) -> String {
    let state = |on: ToolchainActivation| if on.is_on() { ON_LABEL } else { OFF_LABEL };
    match activation {
        GroupActivation::Inherit => format!("{INHERIT_LABEL} ({})", state(global)),
        GroupActivation::On => ON_LABEL.to_owned(),
        GroupActivation::Off => OFF_LABEL.to_owned(),
    }
}

fn is_posix_shell(shell: &str) -> bool {
    Path::new(shell.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| POSIX_SHELLS.contains(&name))
}

fn typed_activation(lines: &[String]) -> Option<String> {
    (!lines.is_empty()).then(|| {
        lines
            .iter()
            .flat_map(|line| [line.as_str(), SHELL_SUBMIT])
            .collect()
    })
}

fn login_shell(link: &HostLink, inputs: &DiscoveryInputs) -> Option<String> {
    if link.is_local() {
        return std::env::var(SHELL_VAR).ok();
    }
    let mut printenv = HostCommand::new(PRINTENV_PROGRAM);
    printenv.arg(SHELL_VAR).in_login_shell();
    let output = link.output_within(&printenv, inputs.timeout).ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

type ActivationKey = (Host, PathBuf);

#[derive(Clone, Debug, Default)]
pub(in crate::environment_shell) struct ActivationCache(
    Arc<Mutex<HashMap<ActivationKey, Vec<String>>>>,
);

impl ActivationCache {
    fn get(&self, key: &ActivationKey) -> Option<Vec<String>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    fn put(&self, key: ActivationKey, lines: Vec<String>) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, lines);
    }

    pub(in crate::environment_shell) fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

fn type_once(writer: &Sender<PtyInput>, first_input: &FirstInput, lines: &[String]) {
    let Some(typed) = typed_activation(lines) else {
        return;
    };
    if !first_input.claim_for_app() {
        tracing::debug!("toolchain activation skipped: the terminal was already in use");
        return;
    }
    drop(writer.send(PtyInput::Bytes(typed.into_bytes())));
}

struct ActivationJob {
    inputs: DiscoveryInputs,
    families: Vec<ToolchainFamilyManifest>,
    shared: Vec<SharedLocatorManifest>,
    cache: Arc<ToolchainCache>,
    choices: ToolchainChoices,
    writer: Sender<PtyInput>,
    first_input: FirstInput,
    activations: ActivationCache,
    key: ActivationKey,
    typed_from_cache: bool,
}

impl ActivationJob {
    fn run(self) {
        let shell = login_shell(&self.inputs.link, &self.inputs);
        if !shell.as_deref().is_some_and(is_posix_shell) {
            tracing::debug!("toolchain activation skipped for a shell that is not POSIX");
            return;
        }
        let root = self.inputs.group_root.clone();
        let mut lines = Vec::new();
        for family in &self.families {
            let marked = family
                .markers
                .iter()
                .any(|marker| self.inputs.link.exists(&root.join(marker)));
            if !marked {
                continue;
            }
            let (scope, discovery) = discover_scope(
                &self.inputs,
                &self.cache,
                family,
                &self.shared,
                &self.choices,
                &ProjectFolder::root(),
            );
            let effective =
                EffectiveToolchain::resolve(&self.choices.selection(&scope), &discovery.candidates);
            if let Some(toolchain) = effective.toolchain() {
                lines.extend(activation_lines(family, toolchain));
            }
        }
        self.activations.put(self.key.clone(), lines.clone());
        if !self.typed_from_cache {
            type_once(&self.writer, &self.first_input, &lines);
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn activation_submenu(
        &self,
        group: &TerminalGroup,
    ) -> MenuEntry {
        let global = self.settings.toolchain_activation;
        let options = GroupActivation::ALL
            .into_iter()
            .map(|activation| {
                MenuEntry::checked(
                    activation_label(activation, global),
                    group.activation == activation,
                    MenuAction::SetGroupActivation(group.id, activation),
                )
            })
            .collect();
        MenuEntry::submenu(ACTIVATION_MENU_LABEL, IconName::Layers, options)
    }

    pub(in crate::environment_shell) fn set_group_activation(
        &mut self,
        id: GroupId,
        activation: GroupActivation,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self
            .envs
            .get_mut(&self.active)
            .and_then(|live| live.data.groups.iter_mut().find(|group| group.id == id))
        else {
            return;
        };
        group.activation = activation;
        self.store();
        cx.notify();
    }

    pub(in crate::environment_shell) fn activate_toolchains(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        writer: Sender<PtyInput>,
        first_input: FirstInput,
        site: ShellSite,
    ) {
        if site.is_wsl() {
            return;
        }
        let Some(live) = self.envs.get(env) else {
            return;
        };
        let Some(group) = live.data.group_of(id) else {
            return;
        };
        if !group
            .activation
            .resolve(self.settings.toolchain_activation)
            .is_on()
        {
            return;
        }
        let Some(tools) = self.language_tools() else {
            return;
        };
        let Some(inputs) = self.discovery_inputs(&live.data.host, group.root.clone()) else {
            return;
        };
        let key = (live.data.host.clone(), group.root.clone());
        let cached = self.toolchains.activations.get(&key);
        if let Some(lines) = &cached {
            type_once(&writer, &first_input, lines);
        }
        let job = ActivationJob {
            inputs,
            families: tools.all_toolchain_families().to_vec(),
            shared: tools.shared_locators().to_vec(),
            cache: Arc::clone(&self.toolchains.cache),
            choices: self.toolchains.choices.clone(),
            writer,
            first_input,
            activations: self.toolchains.activations.clone(),
            key,
            typed_from_cache: cached.is_some(),
        };
        let started = std::thread::Builder::new()
            .name(THREAD_ACTIVATION.to_owned())
            .spawn(move || job.run());
        if let Err(error) = started {
            tracing::warn!(%error, "toolchain activation could not start");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_posix_shells_get_the_posix_activation() {
        assert!(is_posix_shell("/bin/bash"));
        assert!(is_posix_shell("/usr/bin/zsh\n"));
        assert!(is_posix_shell("dash"));
        assert!(!is_posix_shell("/usr/bin/fish"));
        assert!(!is_posix_shell("/bin/tcsh"));
        assert!(!is_posix_shell("nu"));
        assert!(!is_posix_shell(""));
    }

    #[test]
    fn the_inherited_choice_shows_what_settings_say() {
        assert_eq!(
            activation_label(GroupActivation::Inherit, ToolchainActivation::Off),
            "Like Settings (Off)"
        );
        assert_eq!(
            activation_label(GroupActivation::On, ToolchainActivation::Off),
            "On"
        );
    }

    #[test]
    fn each_activation_line_is_submitted_on_its_own() {
        assert_eq!(typed_activation(&[]), None);
        assert_eq!(
            typed_activation(&[
                "export JAVA_HOME='/opt/jdk'".to_owned(),
                "export PATH='/opt/jdk/bin':\"$PATH\"".to_owned(),
            ]),
            Some("export JAVA_HOME='/opt/jdk'\rexport PATH='/opt/jdk/bin':\"$PATH\"\r".to_owned())
        );
    }
}
