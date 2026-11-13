use gpui_kit::Context;
use krad_domain::{
    EnvironmentData, EnvironmentName, EnvironmentViewPreferences, Layout, NodeEdit, NodeField,
    SessionEdit, SidePanelLayout, TerminalId, TerminalNode, ToastKind, ViewPreferences,
    DEFAULT_ENVIRONMENT, LAYOUT_SCHEMA_VERSION,
};
use krad_protocol::Command;

use super::{EnvironmentShell, LiveEnv};

impl EnvironmentShell {
    pub(super) fn restore_layout(&mut self, layout: Layout) {
        let Layout { state, mut view } = layout;
        self.toolchains =
            super::toolchains::Toolchains::with_choices(std::mem::take(&mut view.toolchains));
        let mut environments = state.environments;
        for data in &mut environments {
            data.restart();
        }
        self.runtime.view.environments.replace(environments.clone());
        let mut restored = 0;
        for mut data in environments {
            view.dress(&mut data);
            let ids: Vec<TerminalId> = data.nodes.iter().map(|node| node.id).collect();
            let data_focus = data.focus.filter(|id| ids.contains(id));
            let name = data.name.clone();
            self.order.push(name.clone());
            self.envs
                .insert(name.clone(), LiveEnv::new(data, data_focus));
            if let Some(terminal) = data_focus {
                self.edit_environment(&name, SessionEdit::Reveal { terminal });
            }
            restored += self.spawn_missing_sessions(&name);
        }
        if self.order.is_empty() {
            let data = EnvironmentData::empty(EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned()));
            let name = data.name.clone();
            self.order.push(name.clone());
            self.envs.insert(name.clone(), LiveEnv::new(data, None));
            restored += self.spawn_missing_sessions(&name);
        }
        self.work_mode = view.work_mode;
        self.sidebar_w = view.sidebar_width;
        match view.active.filter(|active| self.envs.contains_key(active)) {
            Some(active) => self.active = active,
            None => {
                if let Some(first) = self.order.first().cloned() {
                    self.active = first;
                }
            }
        }
        if restored > 0 {
            self.push_toast(ToastKind::Success, format!("Restored {restored} terminals"));
        }
    }

    fn snapshot_env(&self, name: &EnvironmentName, live: &LiveEnv) -> EnvironmentData {
        EnvironmentData {
            name: live.data.name.clone(),
            host: live.data.host.clone(),
            nodes: live.data.nodes.clone(),
            meta: live.data.meta.clone(),
            next_id: live.data.next_id,
            view: live.data.view,
            focus: live.focused,
            title: live.data.title.clone(),
            groups: live.data.groups.clone(),
            sessions: live.data.sessions.clone(),
            active_session: live.data.active_session,
            side_panel: self.persisted_side_panel(name, live),
            note_tabs: live.data.note_tabs.clone(),
            extension_views: live.data.extension_views.clone(),
            retired_spend: live.data.retired_spend.clone(),
            variant_runs: live.data.variant_runs.clone(),
            archive: live.data.archive.clone(),
        }
    }

    fn persisted_side_panel(&self, name: &EnvironmentName, live: &LiveEnv) -> SidePanelLayout {
        let mut side_panel = live.data.side_panel.clone();
        if let Some(sign_in) = self.sign_in_terminal_of(name) {
            side_panel.remove_terminal(sign_in);
        }
        for helper in self.runner_helper_terminals(name) {
            side_panel.remove_terminal(helper);
        }
        side_panel
    }

    fn keep_cloud_offsets(&mut self) {
        let moved: Vec<(EnvironmentName, TerminalNode)> = self
            .order
            .iter()
            .filter_map(|name| Some((name, self.envs.get(name)?)))
            .flat_map(|(name, live)| {
                self.nodes_with_offsets(name, &live.data.nodes)
                    .into_iter()
                    .zip(&live.data.nodes)
                    .filter(|(offset, kept)| offset.cloud != kept.cloud)
                    .map(|(offset, _)| (name.clone(), offset))
                    .collect::<Vec<_>>()
            })
            .collect();
        for (env, node) in moved {
            let kept = NodeEdit::Set {
                terminal: node.id,
                field: NodeField::Cloud(node.cloud),
            };
            self.apply_edit(&env, kept);
        }
    }

    fn view_preferences(&self, environments: &[EnvironmentData]) -> ViewPreferences {
        ViewPreferences {
            version: LAYOUT_SCHEMA_VERSION,
            active: Some(self.active.clone()),
            work_mode: self.work_mode,
            sidebar_width: self.sidebar_w,
            toolchains: self.toolchains.choices.clone(),
            environments: environments
                .iter()
                .map(EnvironmentViewPreferences::from)
                .collect(),
        }
    }

    pub(super) fn settle_runtime_on_quit(cx: &mut Context<Self>) {
        cx.on_app_quit(|shell, _| {
            shell.store();
            let settled = shell.runtime.handle.settle();
            async move {
                if settled.await.is_err() {
                    tracing::warn!("the runtime stopped before the layout settled");
                }
            }
        })
        .detach();
    }

    pub(super) fn store(&mut self) {
        self.keep_cloud_offsets();
        let environments: Vec<EnvironmentData> = self
            .order
            .iter()
            .filter_map(|name| Some(self.snapshot_env(name, self.envs.get(name)?)))
            .collect();
        for command in self
            .runtime
            .view
            .environments
            .commands_toward(&environments)
        {
            self.runtime.handle.post(Command::Environments(command));
        }
        if let Some(writer) = &self.view_writer {
            writer.persist(&self.view_preferences(&environments));
        }
    }
}
