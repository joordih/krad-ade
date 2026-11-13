use gpui_kit::Context;
use krad_chat::LiveSession;
use krad_domain::{
    AgentPreset, EnvironmentName, GroupId, NodeField, NodeKind, ResumeId, TerminalId,
};

use super::super::super::live_terminal::end_terminal_on;
use super::super::super::sessions::Launch;
use super::super::super::EnvironmentShell;

impl EnvironmentShell {
    fn node_resume(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<(Option<GroupId>, AgentPreset, ResumeId)> {
        let node = self
            .envs
            .get(env)?
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)?;
        let harness = node.harness.filter(|preset| preset.is_agent())?;
        Some((node.group, harness, node.resume.clone()?))
    }

    pub(in crate::environment_shell) fn continue_in_terminal(
        &mut self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some((group, harness, resume)) = self.node_resume(&env, id) else {
            return;
        };
        self.leave_runtime(&env, id);
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        live.chats.remove(&id);
        live.pending_chats.remove(&id);
        let cwd = group
            .and_then(|group| live.data.group(group))
            .map(|group| group.root.clone());
        self.set_node(&env, id, NodeField::Kind(NodeKind::Terminal));
        self.set_node(&env, id, NodeField::Harness(Some(harness)));
        let launch = Launch::Resume {
            preset: harness,
            resume: Some(resume),
        };
        self.start_session(&env, id, launch, cwd, &[]);
        self.store();
        cx.notify();
    }

    pub(in crate::environment_shell) fn return_to_chat(
        &mut self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some((_, harness, resume)) = self.node_resume(&env, id) else {
            return;
        };
        self.forget_opencode_terminal(&env, id);
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        if live.sessions.remove(&id).is_some() {
            end_terminal_on(self.runtime.client.as_ref(), &env, id);
        }
        live.pending_chats.insert(id, LiveSession::Resume(resume));
        self.set_node(&env, id, NodeField::Kind(NodeKind::Chat));
        self.set_node(&env, id, NodeField::Harness(Some(harness)));
        self.store();
        cx.notify();
    }
}
