use krad_domain::{
    line_diff, ChatEvent, Checklist, ChecklistItem, ChecklistStatus, FileEdit, ToolCall,
    ToolKind as ShownKind, ToolUseId,
};
use std::collections::HashMap;
use std::mem;

use crate::acp::content::ContentBlock;
use crate::acp::ids::ToolCallId;
use crate::acp::update::{
    AvailableCommand, Diff, Plan, PlanEntryStatus, SessionUpdate, ToolCall as AgentToolCall,
    ToolCallContent, ToolCallStatus, ToolCallUpdate, ToolKind,
};
use crate::control::{Catalog, CommandOffer, LiveEvent};
use crate::parse::tool_summary;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ToolState {
    title: String,
    kind: ToolKind,
    status: ToolCallStatus,
    summary: String,
    edit: Option<FileEdit>,
    shown: bool,
}

impl ToolState {
    fn worth_showing(&self) -> bool {
        self.status != ToolCallStatus::Pending || !self.summary.is_empty() || self.edit.is_some()
    }
}

#[derive(Debug, Default)]
pub(super) struct Translator {
    thought: String,
    tools: HashMap<ToolCallId, ToolState>,
    catalog: Catalog,
}

const fn shown_kind(kind: ToolKind) -> ShownKind {
    match kind {
        ToolKind::Read => ShownKind::Read,
        ToolKind::Edit | ToolKind::Delete | ToolKind::Move => ShownKind::Edit,
        ToolKind::Search => ShownKind::Search,
        ToolKind::Execute => ShownKind::Shell,
        ToolKind::Fetch => ShownKind::Fetch,
        ToolKind::Think | ToolKind::SwitchMode | ToolKind::Other => ShownKind::Other,
    }
}

const fn checklist_status(status: PlanEntryStatus) -> ChecklistStatus {
    match status {
        PlanEntryStatus::Pending => ChecklistStatus::Pending,
        PlanEntryStatus::InProgress => ChecklistStatus::InProgress,
        PlanEntryStatus::Completed => ChecklistStatus::Done,
    }
}

fn checklist(plan: Plan) -> Checklist {
    plan.entries
        .into_iter()
        .map(|entry| ChecklistItem {
            text: entry.content,
            status: checklist_status(entry.status),
        })
        .collect()
}

fn diff_edit(diff: &Diff) -> FileEdit {
    FileEdit {
        path: diff.path.display().to_string(),
        lines: line_diff(diff.old_text.as_deref().unwrap_or_default(), &diff.new_text),
    }
}

fn first_edit(content: &[ToolCallContent]) -> Option<FileEdit> {
    content.iter().find_map(|content| match content {
        ToolCallContent::Diff(diff) => Some(diff_edit(diff)),
        ToolCallContent::Content { .. } | ToolCallContent::Terminal { .. } => None,
    })
}

fn command_offer(command: AvailableCommand) -> CommandOffer {
    CommandOffer {
        name: command.name,
        description: command.description,
    }
}

impl Translator {
    pub(super) const fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub(super) fn translate(&mut self, update: SessionUpdate) -> Vec<LiveEvent> {
        if let SessionUpdate::AgentThoughtChunk(chunk) = &update {
            if let ContentBlock::Text(text) = &chunk.content {
                self.thought.push_str(&text.text);
            }
            return Vec::new();
        }
        let mut events = self.thought_ended();
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    events.push(LiveEvent::Chat(ChatEvent::TextDelta(text.text)));
                }
            }
            SessionUpdate::ToolCall(call) => events.extend(self.called(call)),
            SessionUpdate::ToolCallUpdate(update) => events.extend(self.updated(&update)),
            SessionUpdate::Plan(plan) => {
                events.push(LiveEvent::Chat(ChatEvent::Checklist(checklist(plan))));
            }
            SessionUpdate::AvailableCommandsUpdate(update) => {
                self.catalog.commands = update
                    .available_commands
                    .into_iter()
                    .map(command_offer)
                    .collect();
                events.push(LiveEvent::Ready(self.catalog.clone()));
            }
            SessionUpdate::UsageUpdate(usage) => events.extend([
                LiveEvent::Chat(ChatEvent::ContextTokens(usage.used)),
                LiveEvent::Chat(ChatEvent::ContextWindow(usage.size)),
            ]),
            SessionUpdate::AgentThoughtChunk(_)
            | SessionUpdate::UserMessageChunk(_)
            | SessionUpdate::CurrentModeUpdate(_)
            | SessionUpdate::ConfigOptionUpdate(_)
            | SessionUpdate::Unknown => {}
        }
        events
    }

    pub(super) fn turn_ended(&mut self) -> Vec<LiveEvent> {
        let mut events = self.thought_ended();
        let unshown: Vec<ToolCallId> = self
            .tools
            .iter()
            .filter(|(_, state)| !state.shown)
            .map(|(id, _)| id.clone())
            .collect();
        events.extend(unshown.iter().filter_map(|id| self.shown(id)));
        self.tools.clear();
        events
    }

    pub(super) fn asked_tool(&mut self, update: &ToolCallUpdate) -> ToolCall {
        let state = self.merged(update);
        tool_call(&update.tool_call_id, state)
    }

    fn thought_ended(&mut self) -> Vec<LiveEvent> {
        let thought = mem::take(&mut self.thought);
        let thought = thought.trim();
        if thought.is_empty() {
            return Vec::new();
        }
        vec![LiveEvent::Chat(ChatEvent::Thinking(thought.to_owned()))]
    }

    fn called(&mut self, call: AgentToolCall) -> Vec<LiveEvent> {
        let summary = call.locations.first().map_or_else(
            || tool_summary(call.raw_input.as_ref()),
            |location| location.path.display().to_string(),
        );
        let state = ToolState {
            title: call.title,
            kind: call.kind,
            status: call.status,
            summary,
            edit: first_edit(&call.content),
            shown: false,
        };
        let worth_showing = state.worth_showing();
        self.tools.insert(call.tool_call_id.clone(), state);
        if !worth_showing {
            return Vec::new();
        }
        self.shown(&call.tool_call_id).into_iter().collect()
    }

    fn updated(&mut self, update: &ToolCallUpdate) -> Vec<LiveEvent> {
        let had_edit = self
            .tools
            .get(&update.tool_call_id)
            .and_then(|state| state.edit.clone());
        let state = self.merged(update);
        let (shown, worth_showing, edit) = (state.shown, state.worth_showing(), state.edit.clone());
        if !shown {
            if !worth_showing {
                return Vec::new();
            }
            return self.shown(&update.tool_call_id).into_iter().collect();
        }
        match edit {
            Some(edit) if had_edit.as_ref() != Some(&edit) => {
                vec![LiveEvent::Chat(ChatEvent::Edited {
                    tool: ToolUseId::from(update.tool_call_id.0.as_str()),
                    edit,
                })]
            }
            _ => Vec::new(),
        }
    }

    fn merged(&mut self, update: &ToolCallUpdate) -> &ToolState {
        let state = self.tools.entry(update.tool_call_id.clone()).or_default();
        if let Some(title) = &update.title {
            state.title.clone_from(title);
        }
        if let Some(kind) = update.kind {
            state.kind = kind;
        }
        if let Some(status) = update.status {
            state.status = status;
        }
        if let Some(location) = update
            .locations
            .as_ref()
            .and_then(|locations| locations.first())
        {
            state.summary = location.path.display().to_string();
        } else if state.summary.is_empty() {
            state.summary = tool_summary(update.raw_input.as_ref());
        }
        if let Some(edit) = update.content.as_deref().and_then(first_edit) {
            state.edit = Some(edit);
        }
        state
    }

    fn shown(&mut self, id: &ToolCallId) -> Option<LiveEvent> {
        let state = self.tools.get_mut(id)?;
        state.shown = true;
        Some(LiveEvent::Chat(ChatEvent::Tool(tool_call(id, state))))
    }
}

fn tool_call(id: &ToolCallId, state: &ToolState) -> ToolCall {
    ToolCall {
        id: Some(ToolUseId::from(id.0.as_str())),
        name: state.title.clone(),
        kind: shown_kind(state.kind),
        summary: state.summary.clone(),
        edit: state.edit.clone(),
    }
}

#[cfg(test)]
mod tests;
