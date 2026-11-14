use krad_domain::EnvironmentName;
use krad_protocol::AgentRef;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};

const REPLAY_BYTES: usize = 64 * 1024;
const WATCH_BUFFER: usize = 256;
const ESCAPE: u8 = 0x1b;
const LINE_FEED: u8 = b'\n';

#[derive(Debug, Default)]
struct Recent {
    bytes: VecDeque<u8>,
    trimmed: bool,
}

impl Recent {
    fn keep(&mut self, bytes: &[u8]) {
        self.bytes.extend(bytes);
        let over = self.bytes.len().saturating_sub(REPLAY_BYTES);
        self.bytes.drain(..over);
        self.trimmed |= over > 0;
    }

    fn replay(&self) -> Vec<u8> {
        let start = if self.trimmed { self.clean_start() } else { 0 };
        self.bytes.iter().skip(start).copied().collect()
    }

    fn clean_start(&self) -> usize {
        let at = |wanted: u8| self.bytes.iter().position(|byte| *byte == wanted);
        at(ESCAPE)
            .or_else(|| at(LINE_FEED).map(|line| line + 1))
            .unwrap_or(0)
    }
}

#[derive(Debug)]
pub struct TerminalWatch {
    pub replay: Vec<u8>,
    pub bytes: Receiver<Vec<u8>>,
}

#[derive(Debug, Default)]
pub(crate) struct Watchers {
    kept: Option<EnvironmentName>,
    recent: HashMap<AgentRef, Recent>,
    watching: HashMap<AgentRef, Vec<SyncSender<Vec<u8>>>>,
}

impl Watchers {
    pub(crate) fn keep_replays_of(&mut self, environment: EnvironmentName) {
        self.kept = Some(environment);
    }

    pub(crate) fn heard(&mut self, terminal: &AgentRef, bytes: &[u8]) {
        if self.kept.as_ref() == Some(&terminal.environment) {
            self.recent.entry(terminal.clone()).or_default().keep(bytes);
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
            .map_or_else(Vec::new, Recent::replay);
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
        assert_eq!(watchers.watch(&terminal("desk")).replay, Vec::<u8>::new());
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
        assert!(replay.ends_with(b"end"));
        assert!(replay.len() <= REPLAY_BYTES);
    }

    #[test]
    fn a_trimmed_replay_starts_at_an_escape_so_no_sequence_is_cut_in_half() {
        let mut watchers = Watchers::default();
        watchers.keep_replays_of(EnvironmentName::new("runner".to_owned()));
        let mut early = b"\x1b[38;5;196m".to_vec();
        early.extend(vec![b'a'; REPLAY_BYTES - 4]);
        watchers.heard(&terminal("runner"), &early);
        watchers.heard(&terminal("runner"), b"\x1b[0mdone");
        let replay = watchers.watch(&terminal("runner")).replay;
        assert_eq!(replay, b"\x1b[0mdone");
    }

    #[test]
    fn an_untrimmed_replay_is_kept_from_its_first_byte() {
        let mut watchers = Watchers::default();
        watchers.keep_replays_of(EnvironmentName::new("runner".to_owned()));
        watchers.heard(&terminal("runner"), b"plain \x1b[1mbold");
        let replay = watchers.watch(&terminal("runner")).replay;
        assert_eq!(replay, b"plain \x1b[1mbold");
    }
}
