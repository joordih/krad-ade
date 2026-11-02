use std::collections::HashMap;
use std::io::BufRead;
use std::sync::{Mutex, PoisonError};

use super::super::events::{CatalogChange, ServerEvent};

const DATA_PREFIX: &str = "data:";
const MAX_NESTING: usize = 16;

pub(crate) struct Delivery {
    pub(crate) event: ServerEvent,
    pub(crate) nested: bool,
}

pub(crate) type Sink = Box<dyn FnMut(Delivery) + Send>;

type CatalogSink = Box<dyn FnMut() + Send>;

#[derive(Default)]
pub(super) struct Routes {
    pub(super) sinks: HashMap<String, (u64, Sink)>,
    pub(super) parents: HashMap<String, String>,
    pub(super) catalog: Option<CatalogSink>,
    pub(super) next: u64,
}

impl Routes {
    fn root(&self, session: &str) -> String {
        let mut at = session;
        for _ in 0..MAX_NESTING {
            match self.parents.get(at) {
                Some(parent) => at = parent,
                None => break,
            }
        }
        at.to_owned()
    }

    fn deliver(&mut self, event: ServerEvent) {
        if let Some((child, parent)) = event.parent() {
            self.parents.insert(child.to_owned(), parent.to_owned());
        }
        let session = event.session();
        let root = self.root(session);
        let nested = root != session;
        if let Some((_, sink)) = self.sinks.get_mut(&root) {
            sink(Delivery { event, nested });
        }
    }

    fn catalog_changed(&mut self) {
        if let Some(sink) = self.catalog.as_mut() {
            sink();
        }
    }

    fn route(&mut self, data: &str) {
        if let Ok(event) = serde_json::from_str::<ServerEvent>(data) {
            return self.deliver(event);
        }
        if serde_json::from_str::<CatalogChange>(data).is_ok() {
            self.catalog_changed();
        }
    }
}

pub(super) fn pump(reader: impl BufRead, routes: &Mutex<Routes>) {
    for line in reader.lines().map_while(Result::ok) {
        let Some(data) = line.strip_prefix(DATA_PREFIX) else {
            continue;
        };
        routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .route(data.trim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn events_of_a_subagent_reach_the_chat_that_started_it() {
        let routes = Mutex::new(Routes::default());
        let (sender, receiver) = channel();
        routes
            .lock()
            .unwrap_or_else(|_| unreachable!("the fixture is valid"))
            .sinks
            .insert(
                "ses_root".to_owned(),
                (
                    1,
                    Box::new(move |delivery: Delivery| {
                        sender
                            .send(delivery.nested)
                            .unwrap_or_else(|_| unreachable!("the fixture is valid"));
                    }),
                ),
            );
        let feed = concat!(
            "data: {\"type\":\"session.created\",\"data\":{\"sessionID\":\"ses_child\",\"parentID\":\"ses_root\"}}\n\n",
            ": heartbeat\n\n",
            "data: {\"type\":\"session.execution.succeeded\",\"data\":{\"sessionID\":\"ses_child\"}}\n\n",
            "data: {\"type\":\"session.execution.succeeded\",\"data\":{\"sessionID\":\"ses_root\"}}\n\n",
            "data: {\"type\":\"session.execution.succeeded\",\"data\":{\"sessionID\":\"ses_other\"}}\n\n",
            "data: {\"type\":\"websearch.updated\",\"data\":{\"sessionID\":\"ses_root\"}}\n\n",
        );
        pump(feed.as_bytes(), &routes);
        let nested: Vec<bool> = receiver.try_iter().collect();
        assert_eq!(nested, [true, true, false]);
    }

    #[test]
    fn a_change_of_models_or_providers_tells_the_catalog_watcher() {
        let routes = Mutex::new(Routes::default());
        let (sender, receiver) = channel();
        routes
            .lock()
            .unwrap_or_else(|_| unreachable!("the fixture is valid"))
            .catalog = Some(Box::new(move || {
            sender
                .send(())
                .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        }));
        let feed = concat!(
            "data: {\"type\":\"provider.updated\",\"location\":{\"directory\":\"/home\"},\"data\":{}}\n\n",
            "data: {\"type\":\"model.updated\",\"data\":{}}\n\n",
            "data: {\"type\":\"command.updated\",\"data\":{}}\n\n",
            "data: {\"type\":\"session.execution.succeeded\",\"data\":{\"sessionID\":\"ses_root\"}}\n\n",
        );
        pump(feed.as_bytes(), &routes);
        assert_eq!(receiver.try_iter().count(), 2);
    }
}
