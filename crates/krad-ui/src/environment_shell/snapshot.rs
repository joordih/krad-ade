use gpui_kit::{App, Context};
use krad_domain::{
    EnvironmentData, EnvironmentName, EnvironmentViewPreferences, Layout, SessionEdit, TerminalId,
    ToastKind, ViewPreferences, DEFAULT_ENVIRONMENT, LAYOUT_SCHEMA_VERSION,
};
use krad_protocol::{Command, RuntimeCommand};

use super::{EnvironmentShell, LiveEnv};

impl EnvironmentShell {
    pub(super) fn restore_layout(&mut self, layout: Layout, cx: &App) {
        let Layout { state, mut view } = layout;
        self.toolchains =
            super::toolchains::Toolchains::with_choices(std::mem::take(&mut view.toolchains));
        self.backdrop.chosen = std::mem::take(&mut view.backdrops);
        self.backdrop.chosen.adopt_from(&mut self.settings);
        let mut environments = state.environments;
        for data in &mut environments {
            data.restart();
        }
        self.runtime.environments.replace(environments.clone());
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
            restored += self.spawn_missing_sessions(&name, cx);
        }
        if self.order.is_empty() {
            let data = EnvironmentData::empty(EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned()));
            let name = data.name.clone();
            self.order.push(name.clone());
            self.envs.insert(name.clone(), LiveEnv::new(data, None));
            restored += self.spawn_missing_sessions(&name, cx);
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

    fn snapshot_env(live: &LiveEnv) -> EnvironmentData {
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
            side_panel: live.data.side_panel.clone(),
            note_tabs: live.data.note_tabs.clone(),
            extension_views: live.data.extension_views.clone(),
            retired_spend: live.data.retired_spend.clone(),
            variant_runs: live.data.variant_runs.clone(),
            archive: live.data.archive.clone(),
        }
    }

    fn view_preferences(&self, environments: &[EnvironmentData]) -> ViewPreferences {
        ViewPreferences {
            version: LAYOUT_SCHEMA_VERSION,
            active: Some(self.active.clone()),
            work_mode: self.work_mode,
            sidebar_width: self.sidebar_w,
            toolchains: self.toolchains.choices.clone(),
            backdrops: self.backdrop.chosen.clone(),
            environments: environments
                .iter()
                .map(EnvironmentViewPreferences::from)
                .collect(),
        }
    }

    pub(super) fn settle_runtime_on_quit(cx: &mut Context<Self>) {
        cx.on_app_quit(|shell, _| {
            shell.store();
            let settled = shell
                .runtime
                .client
                .send(Command::Runtime(RuntimeCommand::Settle));
            async move {
                if let Err(error) = settled.await {
                    tracing::warn!(%error, "the runtime did not settle the layout");
                }
            }
        })
        .detach();
    }

    pub(super) fn store(&mut self) {
        let environments: Vec<EnvironmentData> = self
            .order
            .iter()
            .filter_map(|name| Some(Self::snapshot_env(self.envs.get(name)?)))
            .collect();
        for command in self.runtime.environments.commands_toward(&environments) {
            self.runtime.client.post(Command::Environments(command));
        }
        if let Some(writer) = &self.view_writer {
            writer.persist(&self.view_preferences(&environments));
        }
    }
}
