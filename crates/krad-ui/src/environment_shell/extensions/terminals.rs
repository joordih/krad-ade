use gpui_kit::{Context, Keystroke};
use krad_domain::{AgentPreset, EnvironmentName, GroupId, TerminalId};
use krad_extensions::api::protocol::{OpenTerminalParams, TerminalZone};
use krad_extensions::api::TerminalHandle;
use krad_session::{KeyModifiers, PtyInput};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use super::super::sessions::Launch;
use super::super::terminal_lifecycle::{NodeGroup, NodeSpec};
use super::super::EnvironmentShell;
use super::SlotKey;

use krad_extensions::api::limits::MAX_TERMINALS_PER_EXTENSION;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::environment_shell) struct OwnedTerminal {
    owner: SlotKey,
    env: EnvironmentName,
    terminal: TerminalId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TerminalRefusal {
    OutsideRoots,
    NoTerminal,
    TooMany,
}

pub(super) fn owned_by(
    terminals: &HashMap<TerminalHandle, OwnedTerminal>,
    key: &SlotKey,
    handle: TerminalHandle,
) -> Option<(EnvironmentName, TerminalId)> {
    terminals
        .get(&handle)
        .filter(|owned| owned.owner == *key)
        .map(|owned| (owned.env.clone(), owned.terminal))
}

fn count_owned(terminals: &HashMap<TerminalHandle, OwnedTerminal>, key: &SlotKey) -> usize {
    terminals
        .values()
        .filter(|owned| owned.owner == *key)
        .count()
}

impl TerminalRefusal {
    pub(super) const fn reason(&self) -> &'static str {
        match self {
            Self::OutsideRoots => {
                "the folder is outside the environment's groups and the extension"
            }
            Self::NoTerminal => "the terminal could not be opened",
            Self::TooMany => "the extension has opened as many terminals as it may",
        }
    }
}

fn plain(path: &Path) -> bool {
    path.components()
        .all(|component| !matches!(component, Component::ParentDir))
}

pub(super) fn contained_cwd(cwd: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    if !plain(cwd) {
        return None;
    }
    let resolved = if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        roots.first()?.join(cwd)
    };
    roots
        .iter()
        .any(|root| resolved.starts_with(root))
        .then_some(resolved)
}

impl EnvironmentShell {
    fn terminal_roots(
        &self,
        key: &SlotKey,
        env: &EnvironmentName,
    ) -> Vec<(Option<GroupId>, PathBuf)> {
        let groups = self.envs.get(env).map_or_else(Vec::new, |live| {
            live.data
                .groups
                .iter()
                .map(|group| (Some(group.id), group.root.clone()))
                .collect()
        });
        let folder = self
            .extensions
            .record(&key.extension)
            .and_then(super::ExtensionRecord::program)
            .map(|program| (None, program.folder().to_path_buf()));
        groups.into_iter().chain(folder).collect()
    }

    pub(super) fn open_extension_terminal(
        &mut self,
        key: &SlotKey,
        params: OpenTerminalParams,
        cx: &mut Context<Self>,
    ) -> Result<TerminalHandle, TerminalRefusal> {
        if count_owned(&self.extensions.terminals, key) >= MAX_TERMINALS_PER_EXTENSION {
            return Err(TerminalRefusal::TooMany);
        }
        let env = key
            .environment
            .clone()
            .unwrap_or_else(|| self.active.clone());
        let placed = match &params.cwd {
            Some(cwd) => {
                let roots = self.terminal_roots(key, &env);
                let paths: Vec<PathBuf> = roots.iter().map(|(_, root)| root.clone()).collect();
                let resolved = contained_cwd(cwd, &paths).ok_or(TerminalRefusal::OutsideRoots)?;
                let group = roots
                    .iter()
                    .find(|(_, root)| resolved.starts_with(root))
                    .and_then(|(group, _)| *group);
                Some((group, resolved))
            }
            None => None,
        };
        let launch = match params.command {
            Some(command) => Launch::Command {
                program: command.program,
                args: command.args,
                vars: command.env.into_iter().collect(),
            },
            None => Launch::Preset(AgentPreset::Shell),
        };
        let spec = NodeSpec {
            kind: krad_domain::NodeKind::Terminal,
            harness: None,
            label: &params.title,
            group: NodeGroup::OrSelected(placed.as_ref().and_then(|(group, _)| *group)),
        };
        let cwd = placed.map(|(_, cwd)| cwd);
        let terminal = self
            .open_terminal_at(&env, &spec, launch, cwd, cx)
            .ok_or(TerminalRefusal::NoTerminal)?;
        self.extensions.next_terminal += 1;
        let handle = TerminalHandle(self.extensions.next_terminal);
        self.extensions.terminals.insert(
            handle,
            OwnedTerminal {
                owner: key.clone(),
                env: env.clone(),
                terminal,
            },
        );
        if params.zone == TerminalZone::Utilities && env == self.active {
            self.dock_in_activity(terminal, cx);
        }
        if params.zone == TerminalZone::Session {
            self.show_sessions_for(&env, cx);
        }
        if params.zone == TerminalZone::View {
            if env == self.active {
                self.edit_panes(cx, |panes| panes.release(terminal));
            }
            self.attach_view_terminal(key, handle, &env, terminal, cx);
        }
        Ok(handle)
    }

    pub(super) fn send_extension_terminal(
        &mut self,
        key: &SlotKey,
        handle: TerminalHandle,
        text: String,
    ) {
        let Some((env, terminal)) = owned_by(&self.extensions.terminals, key, handle) else {
            tracing::debug!(extension = %key.extension, "terminal of another extension refused");
            return;
        };
        self.write_terminal(&env, terminal, text.into_bytes());
    }

    fn write_terminal(&mut self, env: &EnvironmentName, terminal: TerminalId, bytes: Vec<u8>) {
        let Some(session) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.sessions.get_mut(&terminal))
        else {
            return;
        };
        if session.writer_tx.send(PtyInput::Bytes(bytes)).is_err() {
            tracing::debug!("extension terminal writer closed");
        }
    }

    pub(in crate::environment_shell) fn type_into_view_terminal(
        &mut self,
        env: &EnvironmentName,
        terminal: TerminalId,
        keystroke: &Keystroke,
    ) {
        if keystroke.modifiers.platform {
            return;
        }
        let held = KeyModifiers {
            ctrl: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
        };
        let bytes = self
            .envs
            .get_mut(env)
            .and_then(|live| live.sessions.get_mut(&terminal))
            .and_then(|session| {
                session
                    .engine
                    .encode_key(&keystroke.key, keystroke.key_char.as_deref(), held)
            });
        if let Some(bytes) = bytes {
            self.write_terminal(env, terminal, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(extension: &str) -> Result<SlotKey, krad_domain::IdError> {
        Ok(SlotKey {
            extension: extension.parse()?,
            environment: None,
        })
    }

    #[test]
    fn an_extension_cannot_type_into_another_extensions_terminal(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let env = EnvironmentName::new("main".to_owned());
        let mut terminals = HashMap::new();
        let handle = TerminalHandle(1);
        terminals.insert(
            handle,
            OwnedTerminal {
                owner: key("docker")?,
                env: env.clone(),
                terminal: TerminalId(7),
            },
        );
        assert!(owned_by(&terminals, &key("docker")?, handle).is_some());
        assert_eq!(owned_by(&terminals, &key("evil")?, handle), None);
        assert_eq!(
            owned_by(&terminals, &key("docker")?, TerminalHandle(2)),
            None
        );
        assert_eq!(count_owned(&terminals, &key("docker")?), 1);
        assert_eq!(count_owned(&terminals, &key("evil")?), 0);
        Ok(())
    }

    fn roots() -> Vec<PathBuf> {
        vec![PathBuf::from("/work/shop"), PathBuf::from("/ext/docker")]
    }

    #[test]
    fn a_folder_inside_a_group_or_the_extension_is_allowed() {
        assert_eq!(
            contained_cwd(Path::new("/work/shop/api"), &roots()),
            Some(PathBuf::from("/work/shop/api"))
        );
        assert_eq!(
            contained_cwd(Path::new("/ext/docker"), &roots()),
            Some(PathBuf::from("/ext/docker"))
        );
    }

    #[test]
    fn a_relative_folder_lands_in_the_first_group() {
        assert_eq!(
            contained_cwd(Path::new("web"), &roots()),
            Some(PathBuf::from("/work/shop/web"))
        );
    }

    #[test]
    fn folders_elsewhere_or_climbing_out_are_refused() {
        assert_eq!(contained_cwd(Path::new("/etc"), &roots()), None);
        assert_eq!(
            contained_cwd(Path::new("/work/shop/../secrets"), &roots()),
            None
        );
        assert_eq!(contained_cwd(Path::new("../x"), &roots()), None);
        assert_eq!(contained_cwd(Path::new("/work/shopping"), &roots()), None);
        assert_eq!(contained_cwd(Path::new("web"), &[]), None);
    }
}
