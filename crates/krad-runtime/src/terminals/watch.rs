use krad_domain::EnvironmentName;
use krad_protocol::AgentRef;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};

const REPLAY_BYTES: usize = 64 * 1024;
const WATCH_BUFFER: usize = 256;

#[derive(Debug)]
pub struct TerminalWatch {
    pub replay: Vec<u8>,
    pub bytes: Receiver<Vec<u8>>,
}

#[derive(Debug, Default)]
pub(crate) struct Watchers {
    kept: Option<EnvironmentName>,
    recent: HashMap<AgentRef, VecDeque<u8>>,
    watching: HashMap<AgentRef, Vec<SyncSender<Vec<u8>>>>,
}

impl Watchers {
    pub(crate) fn keep_replays_of(&mut self, environment: EnvironmentName) {
        self.kept = Some(environment);
    }

    pub(crate) fn heard(&mut self, terminal: &AgentRef, bytes: &[u8]) {
        if self.kept.as_ref() == Some(&terminal.environment) {
            let recent = self.recent.entry(terminal.clone()).or_default();
            recent.extend(bytes);
            let over = recent.len().saturating_sub(REPLAY_BYTES);
            recent.drain(..over);
        }
        let Some(watchers) = self.watching.get_mut(terminal) else {
            return;
        };
        watchers.retain(|watcher| match watcher.try_send(bytes.to_vec()) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                tracing::debug!("a terminal watcher fell behind and was let go");
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        });
        if watchers.is_empty() {
            self.watching.remove(terminal);
        }
    }

    pub(crate) fn watch(&mut self, terminal: &AgentRef) -> TerminalWatch {
        let (sender, bytes) = sync_channel(WATCH_BUFFER);
        self.watching
            .entry(terminal.clone())
            .or_default()
            .push(sender);
        let replay = self
            .recent
            .get(terminal)
            .map_or_else(Vec::new, |recent| recent.iter().copied().collect());
        TerminalWatch { replay, bytes }
    }

    pub(crate) fn forget(&mut self, terminal: &AgentRef) {
        self.recent.remove(terminal);
        self.watching.remove(terminal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::TerminalId;

    fn terminal(environment: &str) -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new(environment.to_owned()),
            terminal: TerminalId(1),
        }
    }

    #[test]
    fn a_watcher_gets_the_kept_replay_then_every_later_chunk() {
        let mut watchers = Watchers::default();
        watchers.keep_replays_of(EnvironmentName::new("runner".to_owned()));
        watchers.heard(&terminal("runner"), b"$ ls\r\n");
        watchers.heard(&terminal("desk"), b"secret");
        let watch = watchers.watch(&terminal("runner"));
        assert_eq!(watch.replay, b"$ ls\r\n");
        assert!(watchers.watch(&terminal("desk")).replay.is_empty());
        watchers.heard(&terminal("runner"), b"a.txt");
        assert_eq!(watch.bytes.try_recv().ok(), Some(b"a.txt".to_vec()));
    }

    #[test]
    fn the_replay_keeps_only_the_latest_bytes() {
        let mut watchers = Watchers::default();
        watchers.keep_replays_of(EnvironmentName::new("runner".to_owned()));
        watchers.heard(&terminal("runner"), &vec![b'a'; REPLAY_BYTES]);
        watchers.heard(&terminal("runner"), b"end");
        let replay = watchers.watch(&terminal("runner")).replay;
        assert_eq!(replay.len(), REPLAY_BYTES);
        assert!(replay.ends_with(b"end"));
    }
}
