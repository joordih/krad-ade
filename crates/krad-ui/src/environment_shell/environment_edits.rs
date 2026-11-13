use gpui_kit::Context;
use krad_domain::{
    Edited, EnvironmentChange, EnvironmentEdit, EnvironmentName, NodeEdit, NodeField, NodeKind,
    TerminalId,
};
use krad_protocol::{Command, RequestId};

use super::live_env::NodesMoved;
use super::surface::restored_chats;
use super::EnvironmentShell;

impl EnvironmentShell {
    pub(super) fn edit_environment(
        &mut self,
        env: &EnvironmentName,
        edit: impl Into<EnvironmentEdit>,
    ) -> Edited {
        if !self.runtime.view.environments.knows(env) {
            self.store();
        }
        self.apply_edit(env, edit)
    }

    pub(super) fn apply_edit(
        &mut self,
        env: &EnvironmentName,
        edit: impl Into<EnvironmentEdit>,
    ) -> Edited {
        let Some(live) = self.envs.get_mut(env) else {
            return Edited::Ignored;
        };
        let edit = edit.into();
        let request = self.runtime.handle.next_request();
        let edited = live.data.edit(&edit, request);
        drop(
            self.runtime
                .handle
                .send(request, Command::from((env.clone(), edit))),
        );
        edited
    }

    pub(super) fn set_node(
        &mut self,
        env: &EnvironmentName,
        terminal: TerminalId,
        field: NodeField,
    ) {
        self.edit_environment(env, NodeEdit::Set { terminal, field });
    }

    pub(super) fn environment_heard(
        &mut self,
        env: &EnvironmentName,
        changes: &[EnvironmentChange],
        answers: Option<RequestId>,
        cx: &mut Context<Self>,
    ) {
        let Some(runtime) = self.runtime.view.environments.get(env) else {
            return;
        };
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let moved = live.data.heard(changes, answers, runtime);
        self.nodes_moved(env, moved, cx);
    }

    pub(super) fn adopt_runtime_environments(&mut self, cx: &mut Context<Self>) {
        let names: Vec<EnvironmentName> = self.envs.keys().cloned().collect();
        for env in names {
            let Some(runtime) = self.runtime.view.environments.get(&env) else {
                continue;
            };
            let Some(live) = self.envs.get_mut(&env) else {
                continue;
            };
            let moved = live.data.adopt(runtime);
            self.nodes_moved(&env, moved, cx);
        }
    }

    fn nodes_moved(&mut self, env: &EnvironmentName, moved: NodesMoved, cx: &mut Context<Self>) {
        if moved.is_empty() {
            return;
        }
        for gone in moved.gone {
            self.digest_ended_chat(env, gone, cx);
            self.forget_live_node(env, gone, cx);
        }
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let came: Vec<_> = live
            .data
            .nodes
            .iter()
            .filter(|node| moved.came.contains(&node.id) && node.kind == NodeKind::Chat)
            .cloned()
            .collect();
        live.pending_chats.extend(restored_chats(&came));
        cx.notify();
    }
}
