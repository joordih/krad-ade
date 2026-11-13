mod model;
mod view;

use gpui_kit::Context;
use krad_chat::Runner;
use krad_domain::{AgentPreset, AgentStatus, Host, UsageIndicator};
use krad_host::HostLink;
use krad_protocol::{Command, OpencodeStatus, UsageCommand, UsageNote, UsageShown};
use krad_runtime::{SourceState, UsagePlace, UsageSource};
use std::collections::HashMap;
use std::path::PathBuf;

use super::chat::StartError;
use super::EnvironmentShell;

pub(super) use view::USAGE_BAR_H;

const USAGE_HARNESSES: [AgentPreset; 3] = [
    AgentPreset::ClaudeCode,
    AgentPreset::Codex,
    AgentPreset::Opencode,
];
const OPENCODE_FAILED_TEXT: &str = "The opencode server did not start";

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

    pub(super) fn place_usage(&mut self) {
        let place = self.usage_bar_shown().then(|| {
            let link = self.active_link();
            let sources = self
                .usage_harnesses(&link)
                .into_iter()
                .map(|harness| (harness, self.usage_source(&link, harness)))
                .collect();
            UsagePlace {
                host: link.host(),
                sources,
            }
        });
        self.runtime.handle.place_usage(place);
    }

    pub(super) fn refresh_usage(&mut self, cx: &mut Context<Self>) {
        self.place_usage();
        self.runtime
            .handle
            .post(Command::Usage(UsageCommand::Refresh));
        cx.notify();
    }

    fn installed_on(&self, link: &HostLink, harness: AgentPreset) -> bool {
        if link.is_local() {
            return self.native_agents.contains(&harness);
        }
        self.host_agents
            .get(&link.host())
            .is_some_and(|found| found.contains(&harness))
    }

    fn usage_harnesses(&self, link: &HostLink) -> Vec<AgentPreset> {
        let host = link.host();
        USAGE_HARNESSES
            .into_iter()
            .filter(|harness| {
                self.installed_on(link, *harness)
                    || self.runtime.view.usage.knows(&(host.clone(), *harness))
            })
            .collect()
    }

    fn opencode_usage_source(&mut self, link: &HostLink) -> SourceState {
        let host = link.host();
        match self.runtime.view.opencode.of(&host) {
            Some(OpencodeStatus::Ready) => self
                .runtime
                .handle
                .opencode_server(&host)
                .map_or(SourceState::Waiting(UsageNote::Starting), |server| {
                    SourceState::Ready(UsageSource::Opencode(server))
                }),
            Some(OpencodeStatus::Starting) => SourceState::Waiting(UsageNote::Starting),
            Some(OpencodeStatus::Failed { error }) => {
                SourceState::Broken(format!("{OPENCODE_FAILED_TEXT}: {error}"))
            }
            Some(OpencodeStatus::Stopped) | None => {
                self.start_opencode_for_usage(link);
                SourceState::Waiting(UsageNote::Starting)
            }
        }
    }

    fn start_opencode_for_usage(&mut self, link: &HostLink) {
        if link.is_local() {
            return self.ensure_opencode();
        }
        match self.harness_runner(link, AgentPreset::Opencode, None) {
            Ok((runner, cwd)) => self.ensure_host_opencode(runner, cwd),
            Err(error) => {
                tracing::debug!(%error, "opencode cannot start on the host for usage yet");
            }
        }
    }

    fn runner_source(
        &self,
        link: &HostLink,
        harness: AgentPreset,
        source: fn(Runner, PathBuf) -> UsageSource,
    ) -> SourceState {
        match self.harness_runner(link, harness, None) {
            Ok((runner, cwd)) => SourceState::Ready(source(runner, cwd)),
            Err(StartError::Waiting) => SourceState::Waiting(UsageNote::HostNotReady),
            Err(error) => SourceState::Broken(error.to_string()),
        }
    }

    fn usage_source(&mut self, link: &HostLink, harness: AgentPreset) -> SourceState {
        match harness {
            AgentPreset::Opencode => self.opencode_usage_source(link),
            AgentPreset::ClaudeCode => self.runner_source(link, harness, UsageSource::Claude),
            AgentPreset::Codex => self.runner_source(link, harness, UsageSource::Codex),
            AgentPreset::Gateway | AgentPreset::Shell | AgentPreset::Acp(_) => {
                SourceState::Waiting(UsageNote::HostNotReady)
            }
        }
    }

    pub(super) fn statusline_heard(&self, host: Host, status: &AgentStatus) {
        self.runtime
            .handle
            .post(Command::Usage(UsageCommand::Statusline {
                host,
                status: status.clone(),
            }));
    }
}
