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

const CHAT_HARNESSES: [AgentPreset; 2] = [AgentPreset::ClaudeCode, AgentPreset::Opencode];

fn surface_lock(harness: AgentPreset, resume: Option<&ResumeId>) -> Option<SurfaceLock> {
    if !CHAT_HARNESSES.contains(&harness) {
        return Some(SurfaceLock::ChatUnsupported);
    }
    resume.is_none().then_some(SurfaceLock::UnknownResumeId)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct SurfaceSwitch {
    pub(in crate::environment_shell) on_chat: bool,
    pub(in crate::environment_shell) lock: Option<SurfaceLock>,
}

impl SurfaceLock {
    pub(in crate::environment_shell) const fn hint(self) -> &'static str {
        match self {
            Self::ChatUnsupported => "Chat surface is for Claude Code and opencode only",
            Self::UnknownResumeId => "Switch once the agent reports its session",
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
                Some(lock) => MenuEntry::Header(lock.hint().to_owned()),
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
    fn a_claude_code_agent_with_a_resume_id_can_show_its_chat_even_while_busy() {
        let id = resume("4f2c0b7c-a91e");
        assert_eq!(surface_lock(AgentPreset::ClaudeCode, Some(&id)), None);
    }

    #[test]
    fn only_claude_code_and_opencode_have_a_chat_surface() {
        let id = resume("4f2c0b7c-a91e");
        assert_eq!(
            surface_lock(AgentPreset::Codex, Some(&id)),
            Some(SurfaceLock::ChatUnsupported)
        );
        assert_eq!(surface_lock(AgentPreset::Opencode, Some(&id)), None);
    }

    #[test]
    fn an_agent_without_a_resume_id_cannot_show_its_chat() {
        assert_eq!(
            surface_lock(AgentPreset::ClaudeCode, None),
            Some(SurfaceLock::UnknownResumeId)
        );
    }
}
