use gpui_kit::{Context, Window};
use krad_domain::{AgentPreset, TerminalId};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::super::sessions::Launch;
use super::super::terminal_card::home_dir;
use super::super::EnvironmentShell;

const SHELL_LABEL: &str = "Terminal";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn start_system_terminals(&mut self) {
        let env = self.active.clone();
        let missing: Vec<TerminalId> = self.envs.get(&env).map_or_default(|live| {
            live.data
                .side_panel
                .terminals
                .iter()
                .map(|terminal| terminal.id)
                .filter(|id| !live.sessions.contains_key(id))
                .collect()
        });
        for id in missing {
            self.start_session(
                &env,
                id,
                Launch::Preset(AgentPreset::Shell),
                home_dir().map(PathBuf::from),
                &[],
            );
        }
    }

    pub(in crate::environment_shell) fn add_system_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(id) = self.alloc_terminal_id(&env) else {
            return;
        };
        if let Some(live) = self.envs.get_mut(&env) {
            let title = format!("{SHELL_LABEL} · {}", id.as_u64());
            live.data.side_panel.add_terminal(id, title);
        }
        self.start_system_terminals();
        self.show_system_terminal(id, window, cx);
        self.store();
    }

    pub(in crate::environment_shell) fn show_system_terminal(
        &mut self,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.envs.get_mut(&self.active) {
            live.data.side_panel.shown = Some(id);
        }
        self.focus_pane(window, cx);
        self.focus_terminal(id);
        cx.notify();
    }

    pub(in crate::environment_shell) fn close_system_terminal(
        &mut self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        if let Some(session) = live.sessions.remove(&id) {
            session.cancelled.store(true, Ordering::SeqCst);
            if let Some(killer) = session.killer {
                killer.kill();
            }
        }
        live.images.remove(&id);
        live.data.side_panel.remove_terminal(id);
        if live.focused == Some(id) {
            live.focused = live.shown_tab();
        }
        cx.notify();
        self.store();
    }

    pub(in crate::environment_shell) fn system_terminals_here(
        &self,
    ) -> Vec<krad_domain::SystemTerminal> {
        let Some(live) = self.envs.get(&self.active) else {
            return Vec::new();
        };
        let placed = |id: TerminalId| {
            live.activity_docked.contains(&id)
                || live
                    .data
                    .current_session()
                    .is_some_and(|session| session.panes.pane_of(id).is_some())
        };
        live.data
            .side_panel
            .terminals
            .iter()
            .filter(|terminal| !placed(terminal.id))
            .cloned()
            .collect()
    }

    pub(in crate::environment_shell) fn shown_system_terminal(&self) -> Option<TerminalId> {
        let here = self.system_terminals_here();
        let shown = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.side_panel.shown_terminal());
        shown
            .filter(|id| here.iter().any(|terminal| terminal.id == *id))
            .or_else(|| here.first().map(|terminal| terminal.id))
    }
}
