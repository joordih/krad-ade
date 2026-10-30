use krad_domain::{AgentPreset, EnvironmentName, NodeKind, ResumeId, TerminalId};

use super::super::context_menu::{MenuAction, MenuEntry};
use super::super::EnvironmentShell;
use crate::components::IconName;

const SHOW_CHAT_LABEL: &str = "Show as Chat";
const SHOW_TERMINAL_LABEL: &str = "Show Terminal";
const CONTINUE_IN_TERMINAL_LABEL: &str = "Terminal view";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum SurfaceLock {
    ChatUnsupported,
    UnknownResumeId,
}

const CHAT_SURFACE_HINT: &str = "Chat surface is for";
const ONLY_SUFFIX: &str = "only";
const LIST_SEPARATOR: &str = ", ";
const LIST_LAST_SEPARATOR: &str = " and ";
const UNKNOWN_RESUME_HINT: &str = "Switch once the agent reports its session";

fn surface_lock(harness: AgentPreset, resume: Option<&ResumeId>) -> Option<SurfaceLock> {
    if harness.terminal_chat().is_none() {
        return Some(SurfaceLock::ChatUnsupported);
    }
    resume.is_none().then_some(SurfaceLock::UnknownResumeId)
}

fn spoken_list(names: &[&str]) -> String {
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => {
            format!("{}{LIST_LAST_SEPARATOR}{last}", rest.join(LIST_SEPARATOR))
        }
        Some((last, _)) => (*last).to_owned(),
        None => String::new(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct SurfaceSwitch {
    pub(in crate::environment_shell) on_chat: bool,
    pub(in crate::environment_shell) lock: Option<SurfaceLock>,
}

impl SurfaceLock {
    pub(in crate::environment_shell) fn hint(self) -> String {
        match self {
            Self::ChatUnsupported => {
                let names: Vec<&str> = AgentPreset::all()
                    .into_iter()
                    .filter(|preset| preset.terminal_chat().is_some())
                    .map(AgentPreset::display_name)
                    .collect();
                format!("{CHAT_SURFACE_HINT} {} {ONLY_SUFFIX}", spoken_list(&names))
            }
            Self::UnknownResumeId => UNKNOWN_RESUME_HINT.to_owned(),
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn surface_switch_of(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<SurfaceSwitch> {
        let live = self.envs.get(env)?;
        let node = live.data.nodes.iter().find(|node| node.id == id)?;
        if node.kind == NodeKind::Chat {
            return None;
        }
        let harness = node.harness.filter(|preset| preset.is_agent())?;
        Some(SurfaceSwitch {
            on_chat: live.chats.contains_key(&id),
            lock: surface_lock(harness, node.resume.as_ref()),
        })
    }

    pub(in crate::environment_shell) fn surface_menu(&self, id: TerminalId) -> Vec<MenuEntry> {
        let Some(node) = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
        else {
            return Vec::new();
        };
        let resumable = node.resume.is_some();
        let mut entries = Vec::new();
        if node.kind == NodeKind::Chat {
            if resumable {
                entries.push(MenuEntry::item(
                    CONTINUE_IN_TERMINAL_LABEL,
                    Some(IconName::Terminal),
                    MenuAction::ContinueInTerminal(id),
                ));
            }
        } else if let Some(switch) = self.surface_switch_of(&self.active, id) {
            entries.push(match switch.lock {
                None if switch.on_chat => MenuEntry::item(
                    SHOW_TERMINAL_LABEL,
                    Some(IconName::Terminal),
                    MenuAction::SwitchSurface(id),
                ),
                None => MenuEntry::item(
                    SHOW_CHAT_LABEL,
                    Some(IconName::Chat),
                    MenuAction::SwitchSurface(id),
                ),
                Some(lock) => MenuEntry::Header(lock.hint()),
            });
        }
        if !entries.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resume(raw: &str) -> ResumeId {
        raw.parse()
            .unwrap_or_else(|_| unreachable!("{raw} is a valid resume id"))
    }

    #[test]
    fn a_harness_without_a_terminal_chat_is_locked_before_its_resume_id_is_asked() {
        let id = resume("4f2c0b7c-a91e");
        assert_eq!(
            surface_lock(AgentPreset::Shell, Some(&id)),
            Some(SurfaceLock::ChatUnsupported)
        );
        assert_eq!(surface_lock(AgentPreset::Codex, Some(&id)), None);
    }

    #[test]
    fn the_unsupported_hint_names_the_harnesses_in_a_spoken_list() {
        assert_eq!(spoken_list(&["A", "B", "C"]), "A, B and C");
        assert_eq!(spoken_list(&["A"]), "A");
        assert_eq!(
            SurfaceLock::ChatUnsupported.hint(),
            "Chat surface is for Claude Code, Codex and opencode only"
        );
    }

    #[test]
    fn an_agent_without_a_resume_id_cannot_show_its_chat() {
        assert_eq!(
            surface_lock(AgentPreset::ClaudeCode, None),
            Some(SurfaceLock::UnknownResumeId)
        );
    }
}
