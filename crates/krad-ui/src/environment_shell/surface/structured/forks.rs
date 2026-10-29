use gpui_kit::Context;
use krad_domain::{EnvironmentName, GroupId, NodeKind, ResumeId, TerminalId};

use super::super::super::sessions::Launch;
use super::super::super::EnvironmentShell;
use super::super::SURFACE_HARNESS;

impl EnvironmentShell {
    fn node_resume(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<(Option<GroupId>, ResumeId)> {
        let node = self
            .envs
            .get(env)?
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)?;
        Some((node.group, node.resume.clone()?))
    }

    pub(in crate::environment_shell) fn continue_in_terminal(
        &mut self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some((group, resume)) = self.node_resume(&env, id) else {
            return;
        };
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        live.chats.remove(&id);
        live.pending_chats.remove(&id);
        let cwd = group
            .and_then(|group| live.data.group(group))
            .map(|group| group.root.clone());
        if let Some(node) = live.data.nodes.iter_mut().find(|node| node.id == id) {
            node.kind = NodeKind::Terminal;
            node.harness = Some(SURFACE_HARNESS);
        }
        let launch = Launch::Resume {
            preset: SURFACE_HARNESS,
            resume: Some(resume),
        };
        self.start_session(&env, id, launch, cwd, &[]);
        self.store();
        cx.notify();
    }
}
