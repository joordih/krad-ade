use krad_chat::forget_engine_session;
use krad_domain::{AgentPreset, ArchivedChat, EnvironmentName};
use krad_store::{gateway_sessions_dir, ChatImages};

use super::Runtime;

fn forget_files(files: &ChatImages, environment: &EnvironmentName, chat: &ArchivedChat) {
    if let Err(error) = files.forget_archived(environment, chat.terminal) {
        tracing::warn!(chat = chat.terminal.as_u64(), %error, "archived chat files stayed");
    }
    if chat.harness != AgentPreset::Gateway {
        return;
    }
    let Some(sessions) = gateway_sessions_dir() else {
        return;
    };
    if let Err(error) = forget_engine_session(&sessions, &chat.resume) {
        tracing::warn!(chat = chat.terminal.as_u64(), %error, "archived Gateway session stayed");
    }
}

impl Runtime {
    pub(super) fn forget_archived(&self, environment: EnvironmentName, chats: Vec<ArchivedChat>) {
        let Some(files) = self.chat_images.clone().filter(|_| !chats.is_empty()) else {
            return;
        };
        let started = self.jobs.run_reporting(move |_| {
            for chat in &chats {
                forget_files(&files, &environment, chat);
            }
        });
        if let Err(error) = started {
            tracing::warn!(%error, "archived chat files could not be forgotten");
        }
    }
}
