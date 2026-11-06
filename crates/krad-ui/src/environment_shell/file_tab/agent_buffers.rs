use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::StreamExt as _;
use gpui_kit::Context;
use krad_chat::acp::OpenBuffers;
use krad_domain::EnvironmentName;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

use super::super::EnvironmentShell;
use super::Load;

const ASKS_QUEUED: usize = 64;
const ANSWER_WAIT: Duration = Duration::from_secs(2);

enum BufferAsk {
    Unsaved(PathBuf, SyncSender<Option<String>>),
    Written(PathBuf),
}

struct TabBuffers(Sender<BufferAsk>);

impl TabBuffers {
    fn ask(&self, ask: BufferAsk) -> bool {
        let sent = self.0.clone().try_send(ask).is_ok();
        if !sent {
            debug!("the file tabs did not take an agent's question about a file");
        }
        sent
    }
}

impl OpenBuffers for TabBuffers {
    fn unsaved_text(&self, path: &Path) -> Option<String> {
        let (answer, answered) = sync_channel(1);
        if !self.ask(BufferAsk::Unsaved(path.to_path_buf(), answer)) {
            return None;
        }
        answered.recv_timeout(ANSWER_WAIT).ok().flatten()
    }

    fn written(&self, path: &Path, _content: &str) {
        self.ask(BufferAsk::Written(path.to_path_buf()));
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn tab_buffers(
        env: &EnvironmentName,
        cx: &mut Context<Self>,
    ) -> Arc<dyn OpenBuffers> {
        let (asks, asked) = channel(ASKS_QUEUED);
        Self::answer_buffer_asks(env.clone(), asked, cx);
        Arc::new(TabBuffers(asks))
    }

    fn answer_buffer_asks(
        env: EnvironmentName,
        mut asked: Receiver<BufferAsk>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Some(ask) = asked.next().await {
                let answered = this.update(cx, |shell, cx| shell.answer_buffer_ask(&env, ask, cx));
                if answered.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn answer_buffer_ask(&mut self, env: &EnvironmentName, ask: BufferAsk, cx: &mut Context<Self>) {
        match ask {
            BufferAsk::Unsaved(path, answer) => {
                let text = self.unsaved_tab_text(env, &path, cx);
                if answer.try_send(text).is_err() {
                    debug!("an agent stopped waiting for an open file's text");
                }
            }
            BufferAsk::Written(path) => self.reload_written_tab(env, &path, cx),
        }
    }

    fn unsaved_tab_text(
        &self,
        env: &EnvironmentName,
        path: &Path,
        cx: &Context<Self>,
    ) -> Option<String> {
        self.envs
            .get(env)?
            .editors
            .values()
            .filter(|editor| editor.dirty && editor.path == path)
            .find_map(|editor| editor.state.as_ref())
            .map(|state| state.read(cx).value().to_string())
    }

    fn reload_written_tab(&mut self, env: &EnvironmentName, path: &Path, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let mut reloaded = false;
        for editor in live
            .editors
            .values_mut()
            .filter(|editor| editor.path == path)
        {
            editor.load = Load::Pending;
            reloaded = true;
        }
        if reloaded {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn an_unsaved_text_comes_back_from_whoever_answers_the_tabs() {
        let (asks, mut asked) = channel(ASKS_QUEUED);
        let buffers = TabBuffers(asks);
        let tabs = thread::spawn(move || {
            let Some(BufferAsk::Unsaved(path, answer)) = futures::executor::block_on(asked.next())
            else {
                return asked;
            };
            answer
                .try_send(Some(format!("{} unsaved", path.display())))
                .unwrap_or_default();
            asked
        });
        assert_eq!(
            buffers.unsaved_text(Path::new("/w/a.rs")),
            Some("/w/a.rs unsaved".to_owned())
        );
        drop(tabs.join());
    }

    #[test]
    fn when_no_one_answers_the_disk_is_read_instead() {
        let (asks, asked) = channel(ASKS_QUEUED);
        drop(asked);
        let buffers = TabBuffers(asks);
        assert_eq!(buffers.unsaved_text(Path::new("/w/a.rs")), None);
    }
}
