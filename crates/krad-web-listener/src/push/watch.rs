use futures::StreamExt as _;
use krad_domain::{
    AgentState, CloudRunnerId, EnvironmentChange, EnvironmentData, EnvironmentName, RunnerState,
    TerminalId,
};
use krad_protocol::{EventKind, Feed};
use krad_runner::RuntimeServer;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;

use super::reach::Reached;
use super::{Notice, Push};

const WATCH_THREAD: &str = "krad-web-push-watch";
const SEND_THREAD: &str = "krad-web-push-send";
const PENDING_NOTICES: usize = 32;
const LOCAL_RUNTIME: &str = "local";
const RUNNER_PREFIX: &str = "runner:";
const SESSION_FRAGMENT: &str = "/#s=";
const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn encoded(text: &str) -> String {
    text.bytes().fold(String::new(), |mut out, byte| {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
        out
    })
}

fn settled(before: AgentState, after: AgentState) -> Option<&'static str> {
    if before != AgentState::Working {
        return None;
    }
    match after {
        AgentState::WaitingForInput => Some("is waiting for you"),
        AgentState::Done => Some("finished"),
        AgentState::Working | AgentState::Idle => None,
    }
}

fn notice(
    runtime: &str,
    environment: &EnvironmentData,
    terminal: TerminalId,
    what: &str,
) -> Option<Notice> {
    let node = environment.nodes.iter().find(|node| node.id == terminal)?;
    let session = environment
        .session_of(terminal)
        .and_then(|id| environment.session(id));
    let place = session.map_or(String::new(), |session| {
        let key = format!("{runtime}|{}|{}", environment.name.as_str(), session.id.0);
        format!("{SESSION_FRAGMENT}{}", encoded(&key))
    });
    Some(Notice {
        title: session.map_or_else(|| node.title.clone(), |session| session.name.clone()),
        body: format!("{} {what}", node.title),
        tag: format!("{}/{}", environment.name.as_str(), terminal.0),
        url: if place.is_empty() {
            "/".to_owned()
        } else {
            place
        },
    })
}

pub(super) struct Watcher {
    runtime: String,
    environments: HashMap<EnvironmentName, EnvironmentData>,
    pub(super) ready: Vec<CloudRunnerId>,
}

fn reachable(state: &RunnerState) -> bool {
    matches!(
        state,
        RunnerState::Ready { .. } | RunnerState::UpdatePending { .. }
    )
}

impl Watcher {
    pub(super) fn local() -> Self {
        Self::of(LOCAL_RUNTIME.to_owned())
    }

    pub(super) fn runner(runner: CloudRunnerId) -> Self {
        Self::of(format!("{RUNNER_PREFIX}{}", runner.0))
    }

    fn of(runtime: String) -> Self {
        Self {
            runtime,
            environments: HashMap::new(),
            ready: Vec::new(),
        }
    }

    pub(super) fn take(&mut self, feed: Feed) -> Vec<Notice> {
        let event = match feed {
            Feed::Snapshot(snapshot) => {
                self.ready = snapshot
                    .runners
                    .shown
                    .iter()
                    .filter(|shown| reachable(&shown.state))
                    .map(|shown| shown.runner)
                    .collect();
                self.environments = snapshot
                    .environments
                    .into_iter()
                    .map(|environment| (environment.name.clone(), environment))
                    .collect();
                return Vec::new();
            }
            Feed::Event(event) => event,
        };
        match event.kind {
            EventKind::EnvironmentOpened { environment } => {
                self.environments
                    .insert(environment.name.clone(), *environment);
                Vec::new()
            }
            EventKind::EnvironmentClosed { environment } => {
                self.environments.remove(&environment);
                Vec::new()
            }
            EventKind::EnvironmentChanged {
                environment,
                changes,
                ..
            } => self.change(&environment, changes),
            EventKind::RunnerChanged(shown) => {
                self.ready.retain(|runner| *runner != shown.runner);
                if reachable(&shown.state) {
                    self.ready.push(shown.runner);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn change(&mut self, name: &EnvironmentName, changes: Vec<EnvironmentChange>) -> Vec<Notice> {
        let Some(environment) = self.environments.get_mut(name) else {
            return Vec::new();
        };
        let mut settled_nodes = Vec::new();
        for change in changes {
            if let EnvironmentChange::Node { node, .. } = &change {
                let before = environment
                    .nodes
                    .iter()
                    .find(|known| known.id == node.id)
                    .map(|known| known.state);
                if let Some(what) = before.and_then(|before| settled(before, node.state)) {
                    settled_nodes.push((node.id, what));
                }
            }
            environment.apply(change);
        }
        settled_nodes
            .into_iter()
            .filter_map(|(terminal, what)| notice(&self.runtime, environment, terminal, what))
            .collect()
    }
}

fn deliver(push: &Push, notices: &Receiver<Notice>) {
    for notice in notices {
        push.notify(&notice);
    }
}

pub(crate) fn watch(server: &Arc<RuntimeServer>, push: Arc<Push>) -> std::io::Result<()> {
    let (sender, notices) = mpsc::sync_channel(PENDING_NOTICES);
    thread::Builder::new()
        .name(SEND_THREAD.to_owned())
        .spawn(move || deliver(&push, &notices))?;
    let mut feed = server.handle.subscribe();
    let server = Arc::clone(server);
    thread::Builder::new()
        .name(WATCH_THREAD.to_owned())
        .spawn(move || {
            let mut watcher = Watcher::local();
            let reached = Reached::default();
            while let Some(next) = futures::executor::block_on(feed.next()) {
                let notices = watcher.take(next);
                for runner in &watcher.ready {
                    reached.reach(&server, *runner, &sender);
                }
                if !pass_on(&sender, notices) {
                    return;
                }
            }
        })?;
    Ok(())
}

pub(super) fn pass_on(sender: &SyncSender<Notice>, notices: Vec<Notice>) -> bool {
    for notice in notices {
        match sender.try_send(notice) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::debug!("a push notification was dropped, too many pending");
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{MosaicState, Session};
    use krad_protocol::Event;

    fn node(state: &str) -> serde_json::Value {
        serde_json::json!({ "id": 3, "title": "New chat", "state": state, "kind": "Chat", "text_scale": 1.0 })
    }

    fn environment() -> Result<EnvironmentData, Box<dyn std::error::Error>> {
        let tabs = serde_json::to_value(MosaicState::with_terminals(&[TerminalId(3)]))?;
        let session: Session = serde_json::from_value(
            serde_json::json!({ "id": 2, "group": null, "name": "Fix login", "tabs": tabs }),
        )?;
        Ok(serde_json::from_value(serde_json::json!({
            "name": "my work",
            "nodes": [node("Working")],
            "meta": {},
            "next_id": 4,
            "title": null,
            "groups": [],
            "sessions": [session],
        }))?)
    }

    fn changed(state: &str) -> Result<Feed, Box<dyn std::error::Error>> {
        Ok(Feed::Event(Event {
            seq: krad_protocol::Seq(2),
            kind: EventKind::EnvironmentChanged {
                environment: EnvironmentName::new("my work".to_owned()),
                changes: vec![EnvironmentChange::Node {
                    index: 0,
                    node: serde_json::from_value(node(state))?,
                }],
                answers: None,
            },
        }))
    }

    #[test]
    fn a_chat_that_stops_working_to_wait_is_announced_with_its_session(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut watcher = Watcher::local();
        let opened = environment()?;
        watcher.environments.insert(opened.name.clone(), opened);
        let notices = watcher.take(changed("WaitingForInput")?);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].title, "Fix login");
        assert_eq!(notices[0].body, "New chat is waiting for you");
        assert_eq!(notices[0].url, "/#s=local%7Cmy%20work%7C2");
        assert!(watcher.take(changed("Done")?).is_empty());
        Ok(())
    }

    #[test]
    fn a_runner_that_becomes_ready_is_one_to_follow() -> Result<(), Box<dyn std::error::Error>> {
        let mut watcher = Watcher::local();
        let shown: krad_protocol::RunnerShown = serde_json::from_value(
            serde_json::json!({ "runner": 5, "state": { "state": "connecting" } }),
        )?;
        watcher.take(Feed::Event(Event {
            seq: krad_protocol::Seq(3),
            kind: EventKind::RunnerChanged(shown),
        }));
        assert_eq!(watcher.ready, Vec::new());
        let shown: krad_protocol::RunnerShown = serde_json::from_value(
            serde_json::json!({ "runner": 5, "state": { "state": "ready", "linger": "enabled" } }),
        )?;
        watcher.take(Feed::Event(Event {
            seq: krad_protocol::Seq(4),
            kind: EventKind::RunnerChanged(shown),
        }));
        assert_eq!(watcher.ready, vec![CloudRunnerId(5)]);
        Ok(())
    }
}
