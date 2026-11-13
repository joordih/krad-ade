use krad_domain::Settings;
use krad_protocol::{EventKind, Reply, SettingsCommand};
use krad_store::{config_dir, settings_file_in, LayoutWriter};
use std::path::PathBuf;
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
