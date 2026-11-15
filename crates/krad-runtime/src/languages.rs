pub(crate) mod convert;

use krad_domain::{EnvironmentName, Host};
use krad_extensions::{LanguageTools, ServerPlan};
use krad_lsp::{LanguageServer, LspClient};
use krad_protocol::{BufferId, EventKind, ServerShown, ServerState};
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct ServerKey {
    pub(crate) environment: EnvironmentName,
    pub(crate) host: Host,
    pub(crate) root: PathBuf,
    pub(crate) package: String,
}

pub(crate) enum Tools {
    Unloaded,
    Loading,
    Ready(Arc<LanguageTools>),
    Failed,
}

pub(crate) struct Server {
    pub(crate) plan: ServerPlan,
    pub(crate) state: ServerState,
    pub(crate) running: Option<LanguageServer>,
    pub(crate) documents: BTreeSet<BufferId>,
}

#[derive(Clone, Debug)]
pub(crate) struct Document {
    pub(crate) key: ServerKey,
    pub(crate) path: PathBuf,
    pub(crate) language: String,
}

pub(crate) struct Languages {
    pub(crate) tools: Tools,
    pub(crate) servers: HashMap<ServerKey, Server>,
    pub(crate) documents: HashMap<BufferId, Document>,
    pub(crate) waiting: Vec<BufferId>,
}

impl Default for Languages {
    fn default() -> Self {
        Self {
            tools: Tools::Unloaded,
            servers: HashMap::new(),
            documents: HashMap::new(),
            waiting: Vec::new(),
        }
    }
}

impl fmt::Debug for Languages {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Languages")
            .field("servers", &self.servers.len())
            .field("documents", &self.documents.len())
            .finish_non_exhaustive()
    }
}

impl Languages {
    pub(crate) fn tools(&self) -> Option<Arc<LanguageTools>> {
        match &self.tools {
            Tools::Ready(tools) => Some(Arc::clone(tools)),
            Tools::Unloaded | Tools::Loading | Tools::Failed => None,
        }
    }

    pub(crate) fn client_of(&self, buffer: BufferId) -> Option<(LspClient, &Document)> {
        let document = self.documents.get(&buffer)?;
        let server = self.servers.get(&document.key)?;
        let client = server.running.as_ref()?.client.clone();
        Some((client, document))
    }

    pub(crate) fn announce(&self, key: &ServerKey, outbox: &mut Vec<EventKind>) {
        let Some(server) = self.servers.get(key) else {
            return;
        };
        outbox.push(EventKind::LanguageServer(Box::new(ServerShown {
            environment: key.environment.clone(),
            root: key.root.clone(),
            package: key.package.clone(),
            state: server.state.clone(),
        })));
    }

    pub(crate) fn show(
        &mut self,
        key: &ServerKey,
        state: ServerState,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some(server) = self.servers.get_mut(key) else {
            return;
        };
        if server.state == state {
            return;
        }
        server.state = state.clone();
        outbox.push(EventKind::LanguageServer(Box::new(ServerShown {
            environment: key.environment.clone(),
            root: key.root.clone(),
            package: key.package.clone(),
            state,
        })));
    }

    pub(crate) fn snapshot(&self) -> Vec<ServerShown> {
        let mut shown: Vec<ServerShown> = self
            .servers
            .iter()
            .map(|(key, server)| ServerShown {
                environment: key.environment.clone(),
                root: key.root.clone(),
                package: key.package.clone(),
                state: server.state.clone(),
            })
            .collect();
        shown
            .sort_by(|left, right| (&left.root, &left.package).cmp(&(&right.root, &right.package)));
        shown
    }
}

#[must_use]
pub(crate) fn group_root(roots: &[PathBuf], path: &std::path::Path) -> Option<PathBuf> {
    roots
        .iter()
        .filter(|root| path.starts_with(root))
        .max_by_key(|root| root.components().count())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_belongs_to_its_deepest_group_root() {
        let roots = [
            PathBuf::from("/home/dev/app"),
            PathBuf::from("/home/dev/app/crates/core"),
        ];
        let inner = std::path::Path::new("/home/dev/app/crates/core/src/lib.rs");
        assert_eq!(group_root(&roots, inner), Some(roots[1].clone()));
        let outer = std::path::Path::new("/home/dev/app/README.md");
        assert_eq!(group_root(&roots, outer), Some(roots[0].clone()));
        assert_eq!(group_root(&roots, std::path::Path::new("/tmp/x.rs")), None);
    }
}
