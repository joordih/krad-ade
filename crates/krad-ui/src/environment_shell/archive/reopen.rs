use gpui_kit::Context;
use krad_chat::LiveSession;
use krad_domain::{
    AgentPreset, ArchivedSessionId, ArchivedShell, EnvironmentName, GroupId, NodeKind, ResumedChat,
    TerminalId,
};

use super::super::sessions::Launch;
use super::super::sessions_view::show_current_session;
use super::super::terminal_lifecycle::{NodeGroup, NodeSpec};
use super::super::EnvironmentShell;

impl EnvironmentShell {
    fn resume_reopened(&mut self, env: &EnvironmentName, chats: &[ResumedChat]) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        for chat in chats {
            live.pending_chats
                .insert(chat.terminal, LiveSession::Resume(chat.resume.clone()));
        }
        show_current_session(live);
    }

    fn reopen_shell(
        &mut self,
        env: &EnvironmentName,
        group: GroupId,
        shell: ArchivedShell,
        cx: &mut Context<Self>,
    ) {
        let spec = NodeSpec {
            kind: NodeKind::Terminal,
            harness: None,
            label: &shell.title,
            group: NodeGroup::Exactly(Some(group)),
        };
        let launch = Launch::Preset(AgentPreset::Shell);
        let Some(id) = self.open_terminal_at(env, &spec, launch, shell.cwd, cx) else {
            return;
        };
        if let Some(node) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.data.nodes.iter_mut().find(|node| node.id == id))
        {
            node.title = shell.title;
        }
    }

    pub(in crate::environment_shell) fn reopen_archived_session(
        &mut self,
        id: ArchivedSessionId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let group = live
            .data
            .archive
            .sessions
            .iter()
            .find(|session| session.id == id)
            .map(|session| session.group);
        let (Some(group), Some(reopened)) = (group, live.data.reopen_archived_session(id)) else {
            return;
        };
        live.archive_sheet = None;
        self.resume_reopened(&env, &reopened.chats);
        for shell in reopened.shells {
            self.reopen_shell(&env, group, shell, cx);
        }
        for worker in reopened.workers {
            self.reopen_worker(&env, worker, cx);
        }
        if let Some(live) = self.envs.get_mut(&env) {
            live.data.show_session(reopened.session);
            show_current_session(live);
        }
        cx.notify();
        self.store();
    }

    pub(in crate::environment_shell) fn reopen_archived_chat(
        &mut self,
        terminal: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        live.archive_sheet = None;
        if let Some(resumed) = live.data.reopen_archived_chat(terminal) {
            live.focused = Some(resumed.terminal);
            self.resume_reopened(&env, std::slice::from_ref(&resumed));
        }
        cx.notify();
        self.store();
    }

    pub(in crate::environment_shell) fn reopen_from_archive(
        &mut self,
        terminal: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let Some(archive) = self.envs.get(&self.active).map(|live| &live.data.archive) else {
            return;
        };
        if let Some(session) = archive.session_holding(terminal) {
            self.reopen_archived_session(session, cx);
        } else if archive.holds(terminal) {
            self.reopen_archived_chat(terminal, cx);
        }
    }
}
