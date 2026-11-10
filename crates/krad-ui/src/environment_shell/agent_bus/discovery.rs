use gpui_kit::{AppContext as _, Context};
use krad_agents::{ApiDiscovery, ApiFile};
use krad_domain::ApiDiscoveryFile;
use krad_store::config_dir;

use super::super::EnvironmentShell;

#[derive(Debug)]
pub(super) struct LatestWins<T> {
    running: bool,
    next: Option<T>,
}

impl<T> Default for LatestWins<T> {
    fn default() -> Self {
        Self {
            running: false,
            next: None,
        }
    }
}

impl<T> LatestWins<T> {
    fn ask(&mut self, wanted: T) -> Option<T> {
        if self.running {
            self.next = Some(wanted);
            return None;
        }
        self.running = true;
        Some(wanted)
    }

    fn done(&mut self) -> Option<T> {
        let next = self.next.take();
        self.running = next.is_some();
        next
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_api_file(&mut self, cx: &mut Context<Self>) {
        let wanted = self.settings.api_discovery_file;
        let Some(bus) = &mut self.agent_bus else {
            return;
        };
        if let Some(wanted) = bus.api_file.ask(wanted) {
            self.write_api_file(wanted, cx);
        }
    }

    fn write_api_file(&self, wanted: ApiDiscoveryFile, cx: &mut Context<Self>) {
        let Some(bus) = &self.agent_bus else {
            return;
        };
        let file = config_dir().map(|dir| ApiFile::in_config(&dir));
        let discovery = ApiDiscovery::new(bus.endpoint(), bus.started_at);
        let written = cx.background_spawn(async move {
            let Some(file) = file else {
                return;
            };
            let done = match wanted {
                ApiDiscoveryFile::On => file.write(&discovery),
                ApiDiscoveryFile::Off => file.remove(),
            };
            match done {
                Ok(()) => tracing::info!(?wanted, "agents API discovery file updated"),
                Err(error) => tracing::warn!(%error, "agents API discovery file not updated"),
            }
        });
        cx.spawn(async move |this, cx| {
            written.await;
            this.update(cx, |shell, cx| {
                let next = shell.agent_bus.as_mut().and_then(|bus| bus.api_file.done());
                if let Some(next) = next {
                    shell.write_api_file(next, cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn remove_api_file(&self) {
        if self.agent_bus.is_none() {
            return;
        }
        let Some(file) = config_dir().map(|dir| ApiFile::in_config(&dir)) else {
            return;
        };
        if let Err(error) = file.remove_if_written_by(std::process::id()) {
            tracing::warn!(%error, "agents API discovery file left behind");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_write_runs_at_a_time_and_the_last_choice_wins() {
        let mut queue = LatestWins::default();
        assert_eq!(queue.ask(ApiDiscoveryFile::On), Some(ApiDiscoveryFile::On));
        assert_eq!(queue.ask(ApiDiscoveryFile::Off), None);
        assert_eq!(queue.ask(ApiDiscoveryFile::On), None);
        assert_eq!(queue.ask(ApiDiscoveryFile::Off), None);
        assert_eq!(queue.done(), Some(ApiDiscoveryFile::Off));
        assert_eq!(queue.ask(ApiDiscoveryFile::On), None);
        assert_eq!(queue.done(), Some(ApiDiscoveryFile::On));
        assert_eq!(queue.done(), None);
        assert_eq!(
            queue.ask(ApiDiscoveryFile::Off),
            Some(ApiDiscoveryFile::Off)
        );
    }
}
