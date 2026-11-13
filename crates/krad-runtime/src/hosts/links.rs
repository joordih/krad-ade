use krad_domain::{Host, HostTimeouts, InstallId};
use krad_host::mux::PromptSender;
use krad_host::{HostContext, HostLink};
use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

const THREAD_HOST_CLOSE: &str = "krad-runtime-host-close";

#[derive(Debug, Default)]
struct Registry {
    context: HostContext,
    links: HashMap<Host, HostLink>,
}

#[derive(Clone, Debug)]
pub(crate) struct HostLinks(Arc<RwLock<Registry>>);

impl HostLinks {
    pub(crate) fn new(prompts: PromptSender, context: HostContext) -> Self {
        let context = HostContext {
            prompts: Some(prompts),
            ..context
        };
        Self(Arc::new(RwLock::new(Registry {
            context,
            links: HashMap::new(),
        })))
    }

    pub(crate) fn use_timeouts(&self, timeouts: HostTimeouts) {
        let mut registry = self.0.write().unwrap_or_else(PoisonError::into_inner);
        registry.context.timeouts = timeouts;
    }

    pub(crate) fn install_id(&self) -> Option<InstallId> {
        let registry = self.0.read().unwrap_or_else(PoisonError::into_inner);
        registry.context.install_id.clone()
    }

    pub(crate) fn prompts(&self) -> Option<PromptSender> {
        let registry = self.0.read().unwrap_or_else(PoisonError::into_inner);
        registry.context.prompts.clone()
    }

    pub(crate) fn link(&self, host: &Host) -> HostLink {
        if let Some(link) = self
            .0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .links
            .get(host)
        {
            return link.clone();
        }
        let mut registry = self.0.write().unwrap_or_else(PoisonError::into_inner);
        let context = registry.context.clone();
        registry
            .links
            .entry(host.clone())
            .or_insert_with(|| HostLink::new(host, &context))
            .clone()
    }

    pub(crate) fn close(&self, host: &Host) {
        let closed = self
            .0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .links
            .remove(host);
        if let Some(link) = closed {
            close_link(link);
        }
    }

    pub(crate) fn close_all(&self) {
        let closed: Vec<HostLink> = self
            .0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .links
            .drain()
            .map(|(_, link)| link)
            .collect();
        closed.into_iter().for_each(close_link);
    }
}

fn close_link(link: HostLink) {
    let HostLink::Remote(ssh) = link else {
        return;
    };
    let closing = std::thread::Builder::new()
        .name(THREAD_HOST_CLOSE.to_owned())
        .spawn(move || ssh.close());
    if let Err(error) = closing {
        tracing::warn!(%error, "could not close a host connection");
    }
}
