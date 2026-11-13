mod line;
mod vars;

pub use line::{cli_name, AgentRole, BUS_HINT, LEAD_HINT};
pub use vars::local_caller_vars;

pub(crate) use line::{AgentLine, LaunchSite, LineContext};

use krad_domain::{EnvironmentName, LaunchIntent};
use krad_protocol::AgentRef;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, PoisonError, RwLock};

use crate::input::Input;
use crate::terminals::{Shared, TerminalView};

#[derive(Clone, Default)]
pub(crate) struct LaunchedViews(Arc<RwLock<HashMap<AgentRef, TerminalView>>>);

impl std::fmt::Debug for LaunchedViews {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LaunchedViews")
            .finish_non_exhaustive()
    }
}

impl LaunchedViews {
    pub(crate) fn keep(&self, terminal: AgentRef, view: TerminalView) {
        self.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(terminal, view);
    }

    pub(crate) fn take(&self, terminal: &AgentRef) -> Option<TerminalView> {
        self.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(terminal)
    }

    fn forget_environment(&self, environment: &EnvironmentName) {
        self.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|terminal, _| terminal.environment != *environment);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WaitingLaunch {
    pub(crate) terminal: AgentRef,
    pub(crate) intent: LaunchIntent,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) prefill: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct Launches {
    waiting: Vec<WaitingLaunch>,
    pub(crate) memory_tools: bool,
    pub(crate) views: LaunchedViews,
    inbox: SyncSender<Input>,
}

impl Launches {
    pub(crate) const fn new(views: LaunchedViews, inbox: SyncSender<Input>) -> Self {
        Self {
            waiting: Vec::new(),
            memory_tools: false,
            views,
            inbox,
        }
    }

    pub(crate) fn view(&self, terminal: AgentRef) -> Arc<Shared> {
        let shared = Shared::new(self.inbox.clone());
        self.views
            .keep(terminal, TerminalView::new(Arc::clone(&shared)));
        shared
    }

    pub(crate) fn wait(&mut self, launch: WaitingLaunch) {
        if self.is_waiting(&launch.terminal) {
            return;
        }
        tracing::debug!(
            environment = launch.terminal.environment.as_str(),
            terminal = launch.terminal.terminal.as_u64(),
            "a launch waits for its host"
        );
        self.waiting.push(launch);
    }

    pub(crate) fn is_waiting(&self, terminal: &AgentRef) -> bool {
        self.waiting
            .iter()
            .any(|waiting| waiting.terminal == *terminal)
    }

    pub(crate) fn has_waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    pub(crate) fn take_waiting(&mut self) -> Vec<WaitingLaunch> {
        std::mem::take(&mut self.waiting)
    }

    pub(crate) fn forget(&mut self, terminal: &AgentRef) {
        self.waiting.retain(|waiting| waiting.terminal != *terminal);
        drop(self.views.take(terminal));
    }

    pub(crate) fn forget_environment(&mut self, environment: &EnvironmentName) {
        self.waiting
            .retain(|waiting| waiting.terminal.environment != *environment);
        self.views.forget_environment(environment);
    }
}
