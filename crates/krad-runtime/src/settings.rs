use krad_domain::{CloudProcessId, CloudRunnerId, McpProjectTrust, Settings};
use krad_protocol::{EventKind, Reply, SettingsCommand};
use krad_store::{config_dir, settings_file_in, LayoutWriter};
use std::path::{Path, PathBuf};
use std::sync::mpsc::SyncSender;

use crate::input::Input;

#[derive(Clone, Debug, Default)]
pub struct SettingsSetup {
    pub settings: Settings,
    pub file: Option<PathBuf>,
}

impl SettingsSetup {
    #[must_use]
    pub fn in_config_folder(settings: Settings) -> Self {
        Self {
            settings,
            file: config_dir().map(|dir| settings_file_in(&dir)),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppSettings {
    pub(crate) current: Settings,
    writer: Option<LayoutWriter>,
    dirty: bool,
}

impl AppSettings {
    pub(crate) fn start(setup: SettingsSetup, inbox: &SyncSender<Input>) -> Self {
        let inbox = inbox.clone();
        let writer = setup.file.and_then(|file| {
            LayoutWriter::spawn_reporting(file, move |error| {
                if inbox
                    .send(Input::SettingsNotSaved(error.to_string()))
                    .is_err()
                {
                    tracing::trace!("the settings failed to save after the runtime stopped");
                }
            })
        });
        Self {
            current: setup.settings,
            writer,
            dirty: false,
        }
    }

    pub(crate) fn execute(
        &mut self,
        command: SettingsCommand,
        outbox: &mut Vec<EventKind>,
    ) -> Option<Reply> {
        let SettingsCommand::Save { settings } = command else {
            return None;
        };
        if *settings != self.current {
            self.current = (*settings).clone();
            self.dirty = true;
            outbox.push(EventKind::SettingsChanged { settings });
        }
        Some(Reply::Accepted)
    }

    pub(crate) fn trust_mcp_project(
        &mut self,
        folder: &Path,
        trust: McpProjectTrust,
        outbox: &mut Vec<EventKind>,
    ) {
        if self.current.mcp.project_trust(folder) == Some(trust) {
            return;
        }
        self.current.mcp.set_project_trust(folder, trust);
        self.dirty = true;
        outbox.push(EventKind::SettingsChanged {
            settings: Box::new(self.current.clone()),
        });
    }

    pub(crate) fn take_cloud_stops(
        &mut self,
        runner: CloudRunnerId,
        outbox: &mut Vec<EventKind>,
    ) -> Vec<CloudProcessId> {
        let stops = self.current.cloud.take_stops(runner);
        if !stops.is_empty() {
            self.dirty = true;
            outbox.push(EventKind::SettingsChanged {
                settings: Box::new(self.current.clone()),
            });
        }
        stops
    }

    pub(crate) fn queue_cloud_stop(
        &mut self,
        runner: CloudRunnerId,
        process: CloudProcessId,
        outbox: &mut Vec<EventKind>,
    ) {
        self.current.cloud.queue_stop(runner, process);
        self.dirty = true;
        outbox.push(EventKind::SettingsChanged {
            settings: Box::new(self.current.clone()),
        });
    }

    pub(crate) fn not_saved(error: String, outbox: &mut Vec<EventKind>) {
        tracing::warn!(%error, "could not save the settings");
        outbox.push(EventKind::SettingsNotSaved { error });
    }

    pub(crate) fn flush(&mut self) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        if let Some(writer) = &self.writer {
            writer.persist(&self.current);
        }
    }

    pub(crate) fn settle(&mut self, settled: impl FnOnce() + Send + 'static) {
        self.flush();
        match &self.writer {
            Some(writer) => writer.then(settled),
            None => settled(),
        }
    }

    pub(crate) fn snapshot(&self) -> Box<Settings> {
        Box::new(self.current.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn queued_stops_of_a_runner_are_taken_once_and_announced(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut settings = Settings::default();
        let gpu = settings.cloud.add("gpu".parse()?, "me@gpu".parse()?);
        let other = settings.cloud.add("other".parse()?, "me@other".parse()?);
        let process: CloudProcessId = "api-2-a1b2c3".parse()?;
        settings.cloud.queue_stop(gpu, process.clone());
        settings.cloud.queue_stop(other, "web-3-d4e5f6".parse()?);
        let (inbox, _input) = sync_channel(1);
        let mut kept = AppSettings::start(
            SettingsSetup {
                settings,
                file: None,
            },
            &inbox,
        );
        let mut outbox = Vec::new();
        assert_eq!(kept.take_cloud_stops(gpu, &mut outbox), vec![process]);
        assert!(matches!(
            outbox.as_slice(),
            [EventKind::SettingsChanged { .. }]
        ));
        assert_eq!(kept.take_cloud_stops(gpu, &mut outbox), Vec::new());
        assert_eq!(outbox.len(), 1);
        assert_eq!(kept.current.cloud.take_stops(other).len(), 1);
        Ok(())
    }
}
