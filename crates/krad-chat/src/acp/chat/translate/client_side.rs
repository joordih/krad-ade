use krad_domain::{ChatEvent, FileEdit, ToolCall, ToolKind as ShownKind, ToolUseId};

use super::Translator;
use crate::acp::ids::{TerminalId, ToolCallId};
use crate::acp::update::{ToolCallContent, ToolCallStatus};
use crate::control::LiveEvent;

const WRITE_TITLE: &str = "Write";
const WRITE_ID_PREFIX: &str = "acp-write-";

const fn is_open(status: ToolCallStatus) -> bool {
    matches!(status, ToolCallStatus::Pending | ToolCallStatus::InProgress)
}

impl Translator {
    pub(in crate::acp::chat) fn written(&mut self, edit: FileEdit) -> Vec<LiveEvent> {
        let mut events = self.thought_ended();
        let mut open = self.tools.iter().filter(|(_, state)| is_open(state.status));
        let only = match (open.next(), open.next()) {
            (Some((id, _)), None) => Some(id.clone()),
            _ => None,
        };
        let Some(id) = only else {
            events.push(self.standalone_write(edit));
            return events;
        };
        let Some(state) = self.tools.get_mut(&id) else {
            return events;
        };
        state.edit = Some(edit.clone());
        if !state.shown {
            events.extend(self.shown(&id));
            return events;
        }
        events.push(LiveEvent::Chat(ChatEvent::Edited {
            tool: ToolUseId::from(id.0.as_str()),
            edit,
        }));
        events
    }

    pub(in crate::acp::chat) fn take_terminals(&mut self) -> Vec<(ToolUseId, TerminalId)> {
        std::mem::take(&mut self.watching)
    }

    pub(super) fn watch_terminals(&mut self, id: &ToolCallId, content: &[ToolCallContent]) {
        let Some(state) = self.tools.get_mut(id) else {
            return;
        };
        for content in content {
            let ToolCallContent::Terminal { terminal_id } = content else {
                continue;
            };
            if state.terminals.contains(terminal_id) {
                continue;
            }
            state.terminals.push(terminal_id.clone());
            self.watching
                .push((ToolUseId::from(id.0.as_str()), terminal_id.clone()));
        }
    }

    fn standalone_write(&mut self, edit: FileEdit) -> LiveEvent {
        self.written += 1;
        let id = format!("{WRITE_ID_PREFIX}{}", self.written);
        LiveEvent::Chat(ChatEvent::Tool(ToolCall {
            id: Some(ToolUseId::from(id.as_str())),
            name: WRITE_TITLE.to_owned(),
            kind: ShownKind::Edit,
            summary: edit.path.clone(),
            edit: Some(edit),
        }))
    }
}
