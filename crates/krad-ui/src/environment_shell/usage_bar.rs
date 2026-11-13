mod model;
mod view;

use gpui_kit::{App, Context};
use krad_domain::{AgentPreset, AgentStatus, Host, UsageIndicator};
use krad_protocol::{Command, HarnessSpot, UsageCommand, UsageShown};
use std::collections::HashMap;

use super::EnvironmentShell;

pub(super) use view::USAGE_BAR_H;

const USAGE_HARNESSES: [AgentPreset; 3] = [
    AgentPreset::ClaudeCode,
    AgentPreset::Codex,
    AgentPreset::Opencode,
];

type UsageKey = (Host, AgentPreset);

#[derive(Default)]
pub(super) struct UsageMirror(HashMap<UsageKey, UsageShown>);

impl UsageMirror {
    pub(super) fn apply(&mut self, shown: UsageShown) {
        self.0.insert((shown.host.clone(), shown.harness), shown);
    }

    pub(super) fn replace(&mut self, shown: Vec<UsageShown>) {
        self.0.clear();
        for shown in shown {
            self.apply(shown);
        }
    }

    fn get(&self, key: &UsageKey) -> Option<&UsageShown> {
        self.0.get(key)
    }

    fn knows(&self, key: &UsageKey) -> bool {
        self.0.get(key).is_some_and(|shown| shown.reading.is_some())
    }
}

impl EnvironmentShell {
    pub(super) fn usage_bar_shown(&self) -> bool {
        self.settings.plan_usage.indicator == UsageIndicator::Shown
    }

    pub(super) fn usage_bar_room(&self) -> f32 {
        if self.usage_bar_shown() {
            USAGE_BAR_H
        } else {
            0.0
        }
    }

    pub(super) fn toggle_usage_bar(&mut self, cx: &mut Context<Self>) {
        self.settings.plan_usage.indicator = if self.usage_bar_shown() {
            UsageIndicator::Hidden
        } else {
            UsageIndicator::Shown
        };
        self.save_settings(cx);
        self.place_usage();
        cx.notify();
    }

    pub(super) fn place_usage(&self) {
        let spot = self
            .usage_bar_shown()
            .then(|| HarnessSpot::at(self.active.clone(), None));
        self.runtime
            .client
            .post(Command::Usage(UsageCommand::Place { spot }));
    }

    pub(super) fn refresh_usage(&mut self, cx: &mut Context<Self>) {
        self.place_usage();
        self.runtime
            .client
            .post(Command::Usage(UsageCommand::Refresh));
        cx.notify();
    }

    fn installed_on(&self, host: &Host, harness: AgentPreset) -> bool {
        if host.is_local() {
            return self.native_agents.contains(&harness);
        }
        self.host_agents
            .get(host)
            .is_some_and(|found| found.contains(&harness))
    }

    fn usage_harnesses(&self, host: &Host, cx: &App) -> Vec<AgentPreset> {
        let usage = &self.runtime.view.read(cx).usage;
        USAGE_HARNESSES
            .into_iter()
            .filter(|harness| {
                self.installed_on(host, *harness) || usage.knows(&(host.clone(), *harness))
            })
            .collect()
    }

    pub(super) fn statusline_heard(&self, host: Host, status: &AgentStatus) {
        self.runtime
            .client
            .post(Command::Usage(UsageCommand::Statusline {
                host,
                status: status.clone(),
            }));
    }
}
