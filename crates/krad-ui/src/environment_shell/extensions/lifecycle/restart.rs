use gpui_kit::Context;
use krad_extensions::program::RestartPolicy;

use super::super::super::EnvironmentShell;
use super::super::{Slot, SlotKey};

const GAVE_UP_TEXT: &str = "stopped too often and was not restarted";

impl EnvironmentShell {
    pub(super) fn program_stopped(&mut self, key: &SlotKey, cx: &mut Context<Self>) {
        let running = match self.extensions.slots.remove(key) {
            Some(Slot::Running(running) | Slot::Initializing(running, _)) => running,
            other => {
                if let Some(slot) = other {
                    self.extensions.slots.insert(key.clone(), slot);
                }
                return;
            }
        };
        let ran_for = running.process.ran_for();
        drop(running);
        self.extensions.progress.forget(key);
        tracing::info!(extension = %key.extension, "extension program stopped");
        let delay = self
            .extensions
            .restarts
            .entry(key.clone())
            .or_default()
            .after_crash(ran_for);
        let Some(delay) = delay else {
            let title = self.extensions.title_of(&key.extension);
            self.toast(&format!("{title} {GAVE_UP_TEXT}"), cx);
            self.extensions
                .slots
                .insert(key.clone(), Slot::Failed(GAVE_UP_TEXT.to_owned()));
            self.fail_views_of(key, cx);
            return;
        };
        self.extensions
            .slots
            .insert(key.clone(), Slot::Starting(Vec::new()));
        let restarting = key.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            this.update(cx, |shell, cx| {
                if matches!(shell.extensions.slots.get(&restarting), Some(Slot::Starting(waiting)) if waiting.is_empty())
                {
                    shell.extensions.slots.remove(&restarting);
                    shell.start_program(restarting, Vec::new(), cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn retry_extension(
        &mut self,
        id: &krad_extensions::api::ExtensionId,
        cx: &mut Context<Self>,
    ) {
        let failed: Vec<SlotKey> = self
            .extensions
            .slots
            .iter()
            .filter(|(key, slot)| {
                key.extension == *id && matches!(slot, Slot::Failed(_) | Slot::Unsupported)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in &failed {
            self.extensions.slots.remove(key);
            self.extensions
                .restarts
                .insert(key.clone(), RestartPolicy::default());
        }
        let key = failed.into_iter().next().or_else(|| {
            self.extensions
                .record(id)
                .and_then(super::super::ExtensionRecord::manifest)
                .and_then(|manifest| self.slot_key(manifest))
        });
        if let Some(key) = key {
            self.start_program(key, Vec::new(), cx);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn resume_extension_starts(&mut self, cx: &mut Context<Self>) {
        let waiting: Vec<SlotKey> = self
            .extensions
            .slots
            .iter()
            .filter(|(_, slot)| matches!(slot, Slot::WaitingForTools(_)))
            .map(|(key, _)| key.clone())
            .collect();
        for key in waiting {
            if let Some(Slot::WaitingForTools(deferred)) = self.extensions.slots.remove(&key) {
                self.start_program(key, deferred, cx);
            }
        }
    }
}
