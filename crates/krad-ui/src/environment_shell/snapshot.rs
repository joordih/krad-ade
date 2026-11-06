use krad_domain::{
    AgentState, EnvironmentData, EnvironmentName, PersistedLayout, SidePanelLayout, TerminalId,
    DEFAULT_ENVIRONMENT, LAYOUT_SCHEMA_VERSION,
};

use super::{live_meta, EnvironmentShell, LiveEnv};

impl EnvironmentShell {
    pub(super) fn restore_snapshot(&mut self, snapshot: PersistedLayout) {
        self.toolchains = super::toolchains::Toolchains::with_choices(snapshot.toolchains);
        if !snapshot.unreadable.is_empty() {
            tracing::warn!(
                count = snapshot.unreadable.len(),
                "kept environments this version cannot read; they stay in the layout file"
            );
        }
        self.unreadable_environments = snapshot.unreadable;
        let mut restored = 0;
        for mut data in snapshot.environments {
            let ids: Vec<TerminalId> = data.nodes.iter().map(|node| node.id).collect();
            let data_focus = data.focus.filter(|id| ids.contains(id));
            for node in &mut data.nodes {
                node.state = AgentState::Idle;
            }
            let name = data.name.clone();
            self.order.push(name.clone());
            let link = self.link_for(&data.host);
            self.envs
                .insert(name.clone(), LiveEnv::new(data, data_focus, link));
            restored += self.spawn_missing_sessions(&name);
        }
        if self.order.is_empty() {
            let data = EnvironmentData::empty(EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned()));
            let name = data.name.clone();
            self.order.push(name.clone());
            let link = self.link_for(&data.host);
            self.envs
                .insert(name.clone(), LiveEnv::new(data, None, link));
            restored += self.spawn_missing_sessions(&name);
        }
        self.work_mode = snapshot.work_mode;
        self.sidebar_w = snapshot.sidebar_width;
        let active = snapshot.active;
        if self.envs.contains_key(&active) {
            self.active = active;
        } else if let Some(first) = self.order.first().cloned() {
            self.active = first;
        }
        if restored > 0 {
            self.push_toast(format!("Restored {restored} terminals"));
        }
    }

    pub(super) fn restore_env(&mut self, data: EnvironmentData) {
        let name = data.name.clone();
        self.order.push(name.clone());
        let link = self.link_for(&data.host);
        self.envs
            .insert(name.clone(), LiveEnv::new(data, None, link));
        self.active = name.clone();
        self.spawn_missing_sessions(&name);
    }
    fn snapshot_env(&self, name: &EnvironmentName, live: &LiveEnv) -> EnvironmentData {
        let meta = if *name == self.active {
            live_meta(live)
        } else {
            live.data.meta.clone()
        };
        EnvironmentData {
            name: live.data.name.clone(),
            host: live.data.host.clone(),
            nodes: live.data.nodes.clone(),
            meta,
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
        }
    }

    fn persisted_side_panel(&self, name: &EnvironmentName, live: &LiveEnv) -> SidePanelLayout {
        let mut side_panel = live.data.side_panel.clone();
        if let Some(sign_in) = self.sign_in_terminal_of(name) {
            side_panel.remove_terminal(sign_in);
        }
        side_panel
    }

    fn snapshot(&self) -> PersistedLayout {
        let mut environments = Vec::new();
        for name in &self.order {
            if let Some(live) = self.envs.get(name) {
                environments.push(self.snapshot_env(name, live));
            }
        }
        PersistedLayout {
            version: LAYOUT_SCHEMA_VERSION,
            active: self.active.clone(),
            environments,
            unreadable: self.unreadable_environments.clone(),
            work_mode: self.work_mode,
            sidebar_width: self.sidebar_w,
            toolchains: self.toolchains.choices.clone(),
        }
    }

    pub(super) fn store(&self) {
        if let Some(writer) = &self.layout_writer {
            writer.persist(&self.snapshot());
        }
    }
}
