use krad_domain::{AgentPreset, ChatOrigin};
use std::cell::Cell;
use std::time::Instant;

use super::super::ask::AskQueue;
use super::super::chat::ChatMessage;
use super::files_restore::FilesUndo;
use super::subagents::Subagents;
use super::transcript::TRANSCRIPT_HARNESS;
use super::{Backend, StructuredChat};

pub(in crate::environment_shell) struct Chat {
    pub(in crate::environment_shell) messages: Vec<ChatMessage>,
    pub(in crate::environment_shell) origin: Option<ChatOrigin>,
    pub(in crate::environment_shell) working_since: Cell<Option<Instant>>,
    pub(in crate::environment_shell) problem: Option<String>,
    pub(in crate::environment_shell) backend: Backend,
    pub(super) subagents: Subagents,
    pub(super) files_undo: Option<FilesUndo>,
}

impl Chat {
    pub(super) fn new(backend: Backend) -> Self {
        Self {
            messages: Vec::new(),
            origin: None,
            working_since: Cell::new(None),
            problem: None,
            backend,
            subagents: Subagents::default(),
            files_undo: None,
        }
    }

    pub(in crate::environment_shell) const fn harness(&self) -> AgentPreset {
        match &self.backend {
            Backend::Structured(structured) => structured.harness,
            Backend::Transcript(_) => TRANSCRIPT_HARNESS,
        }
    }

    pub(in crate::environment_shell) const fn structured(&self) -> Option<&StructuredChat> {
        match &self.backend {
            Backend::Structured(structured) => Some(structured),
            Backend::Transcript(_) => None,
        }
    }

    pub(in crate::environment_shell) const fn asks(&self) -> &AskQueue {
        match &self.backend {
            Backend::Structured(structured) => &structured.asks,
            Backend::Transcript(view) => &view.asks,
        }
    }

    pub(in crate::environment_shell) fn asks_mut(&mut self) -> &mut AskQueue {
        match &mut self.backend {
            Backend::Structured(structured) => &mut structured.asks,
            Backend::Transcript(view) => &mut view.asks,
        }
    }

    pub(in crate::environment_shell) fn structured_mut(&mut self) -> Option<&mut StructuredChat> {
        match &mut self.backend {
            Backend::Structured(structured) => Some(structured),
            Backend::Transcript(_) => None,
        }
    }
}
