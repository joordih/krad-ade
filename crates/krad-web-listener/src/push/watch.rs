use futures::StreamExt as _;
use krad_domain::{AgentState, EnvironmentChange, EnvironmentData, EnvironmentName, TerminalId};
use krad_protocol::{EventKind, Feed};
use krad_runner::RuntimeServer;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, TrySendError};
use std::sync::Arc;
use std::thread;

use super::{Notice, Push};

const WATCH_THREAD: &str = "krad-web-push-watch";
const SEND_THREAD: &str = "krad-web-push-send";
const PENDING_NOTICES: usize = 32;
const LOCAL_RUNTIME: &str = "local";
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

fn notice(environment: &EnvironmentData, terminal: TerminalId, what: &str) -> Option<Notice> {
    let node = environment.nodes.iter().find(|node| node.id == terminal)?;
    let session = environment
        .session_of(terminal)
        .and_then(|id| environment.session(id));
    let place = session.map_or(String::new(), |session| {
        let key = format!(
            "{LOCAL_RUNTIME}|{}|{}",
            environment.name.as_str(),
            session.id.0
        );
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

#[derive(Default)]
struct Watcher {
    environments: HashMap<EnvironmentName, EnvironmentData>,
}

impl Watcher {
    fn take(&mut self, feed: Feed) -> Vec<Notice> {
        let event = match feed {
            Feed::Snapshot(snapshot) => {
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
            .filter_map(|(terminal, what)| notice(environment, terminal, what))
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
    thread::Builder::new()
        .name(WATCH_THREAD.to_owned())
        .spawn(move || {
            let mut watcher = Watcher::default();
            while let Some(next) = futures::executor::block_on(feed.next()) {
                for notice in watcher.take(next) {
                    match sender.try_send(notice) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => {
                            tracing::debug!("a push notification was dropped, too many pending");
                        }
                        Err(TrySendError::Disconnected(_)) => return,
                    }
                }
            }
        })?;
    Ok(())
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
        let mut watcher = Watcher::default();
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
}
