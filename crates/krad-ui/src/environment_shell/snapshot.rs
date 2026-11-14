use gpui_kit::Context;
use krad_domain::{
    EnvironmentData, EnvironmentName, EnvironmentViewPreferences, Layout, ToastKind,
    ViewPreferences, DEFAULT_ENVIRONMENT, LAYOUT_SCHEMA_VERSION,
};
use krad_protocol::{Command, RuntimeCommand};
use std::rc::Rc;

use super::EnvironmentShell;

impl EnvironmentShell {
    pub(super) fn restore_layout(&mut self, layout: Layout, cx: &mut Context<Self>) {
        let Layout { state, mut view } = layout;
        self.toolchains =
            super::toolchains::Toolchains::with_choices(std::mem::take(&mut view.toolchains));
        self.backdrop.chosen = std::mem::take(&mut view.backdrops);
        let mut chosen = std::mem::take(&mut self.backdrop.chosen);
        self.edit_settings(cx, |settings| chosen.adopt_from(settings));
        self.backdrop.chosen = chosen;
        let mut environments = state.environments;
        for data in &mut environments {
            data.restart();
        }
        self.runtime.view.update(cx, |model, _| {
            Rc::make_mut(&mut model.environments).replace(environments.clone());
            model.shown.replace(environments);
        });
        self.runtime.reread(cx);
        let mut restored = 0;
        for name in self.runtime.order.clone() {
            let preferences = view.take(&name);
            self.attach_live_env(&name, preferences, cx);
            restored += self.spawn_missing_sessions(&name, cx);
        }
        if self.runtime.order.is_empty() {
            let name = EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned());
            self.open_environment(EnvironmentData::empty(name.clone()), cx);
            restored += self.spawn_missing_sessions(&name, cx);
        }
        self.work_mode = view.work_mode;
        self.sidebar_w = view.sidebar_width;
        match view.active.filter(|active| self.envs.contains_key(active)) {
            Some(active) => self.active = active,
            None => {
                if let Some(first) = self.runtime.order.first().cloned() {
                    self.active = first;
                }
            }
        }
        if restored > 0 {
            self.push_toast(ToastKind::Success, format!("Restored {restored} terminals"));
        }
    }

    fn view_preferences(&self) -> ViewPreferences {
        ViewPreferences {
            version: LAYOUT_SCHEMA_VERSION,
            active: Some(self.active.clone()),
            work_mode: self.work_mode,
            sidebar_width: self.sidebar_w,
            toolchains: self.toolchains.choices.clone(),
            backdrops: self.backdrop.chosen.clone(),
            environments: self
                .runtime
                .order
                .iter()
                .filter_map(|name| self.envs.get(name))
                .map(|live| EnvironmentViewPreferences {
                    focus: live.focused,
                    ..live.view.clone()
                })
                .collect(),
        }
    }

    pub(super) fn settle_runtime_on_quit(cx: &mut Context<Self>) {
        cx.on_app_quit(|shell, _| {
            shell.save_view();
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

    pub(super) fn save_view(&self) {
        if let Some(writer) = &self.view_writer {
            writer.persist(&self.view_preferences());
        }
    }
}
