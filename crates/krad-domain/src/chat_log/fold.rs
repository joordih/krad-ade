use crate::agent::AgentPreset;
use crate::chat::{ChatEntry, ChatEvent, ToolCall, ToolOutput, ToolUseId};
use crate::file_edit::FileEdit;
use crate::mention::{Mention, Prompt};
use crate::permission_mode::PermissionMode;

use super::{AnswerPart, ChatMessage, ShownOutput, Speaker};

const BLOCK_SEPARATOR: &str = "\n\n";

fn append_text(answer: &mut String, piece: &str, whole_blocks: bool) {
    if whole_blocks && !answer.is_empty() {
        answer.push_str(BLOCK_SEPARATOR);
    }
    answer.push_str(piece);
}

fn last_user(messages: &[ChatMessage]) -> Option<usize> {
    messages
        .iter()
        .rposition(|message| message.speaker == Speaker::User)
}

pub fn fold_prompt(
    messages: &mut Vec<ChatMessage>,
    prompt: &Prompt,
    preset: AgentPreset,
) -> Option<usize> {
    let touched = fold_entry(
        messages,
        &ChatEntry::User {
            text: prompt.text.clone(),
            preset,
            model: None,
            images: Vec::new(),
        },
    );
    let asked = last_user(messages)?;
    messages[asked].mentions.clone_from(&prompt.mentions);
    Some(touched.map_or(asked, |touched| touched.min(asked)))
}

pub fn fold_entry(messages: &mut Vec<ChatMessage>, entry: &ChatEntry) -> Option<usize> {
    match entry {
        ChatEntry::User {
            text,
            preset: _,
            model: _,
            images,
        } => {
            let first = messages.len().saturating_sub(1);
            if let Some(last) = messages.last_mut() {
                last.running = false;
            }
            let shown: Vec<Mention> = images.iter().cloned().map(Mention::Image).collect();
            let mut asked = ChatMessage::new(Speaker::User, text.clone());
            asked.mentions = shown;
            let mut answer = ChatMessage::new(Speaker::Assistant, String::new());
            answer.running = true;
            messages.push(asked);
            messages.push(answer);
            Some(first)
        }
        ChatEntry::Event(ChatEvent::Edited { tool, edit }) => place_edit(messages, tool, edit),
        ChatEntry::Event(ChatEvent::ToolOutput { tool, output }) => {
            place_output(messages, tool, output)
        }
        ChatEntry::Event(ChatEvent::Prompted(point)) => {
            let asked = last_user(messages)?;
            messages[asked].rewind = Some(point.clone());
            Some(asked)
        }
        ChatEntry::Event(ChatEvent::TurnStarted) => Some(reopen_answer(messages)),
        ChatEntry::Asked(outcome) => {
            let last = messages.len().checked_sub(1)?;
            let answer = messages.last_mut().filter(|answer| answer.running)?;
            answer.parts.push(AnswerPart::Ask(outcome.clone()));
            Some(last)
        }
        ChatEntry::Event(event) => fold_into_answer(messages, event),
    }
}

fn fold_into_answer(messages: &mut [ChatMessage], event: &ChatEvent) -> Option<usize> {
    let answered = messages.len().checked_sub(1)?;
    let answer = messages.last_mut().filter(|answer| answer.running)?;
    match event {
        ChatEvent::Session(_)
        | ChatEvent::Model(_)
        | ChatEvent::PermissionMode(_)
        | ChatEvent::ContextTokens(_)
        | ChatEvent::ContextWindow(_)
        | ChatEvent::Prompted(_)
        | ChatEvent::TurnStarted
        | ChatEvent::Edited { .. }
        | ChatEvent::ToolOutput { .. } => return None,
        ChatEvent::Checklist(list) => answer.checklist = Some(list.clone()),
        ChatEvent::Text(piece) | ChatEvent::TextDelta(piece) => {
            let whole_blocks = matches!(event, ChatEvent::Text(_));
            append_text(&mut answer.text, piece, whole_blocks);
            match answer.parts.last_mut() {
                Some(AnswerPart::Text(text)) => append_text(text, piece, whole_blocks),
                _ => answer.parts.push(AnswerPart::Text(piece.clone())),
            }
        }
        ChatEvent::Thinking(thought) => {
            answer.parts.push(AnswerPart::Thinking(thought.clone()));
        }
        ChatEvent::Tool(call) => answer.parts.push(AnswerPart::Tool(call.clone())),
        ChatEvent::ToolUpdated(call) => update_tool(&mut answer.parts, call),
        ChatEvent::Done => answer.running = false,
        ChatEvent::Error(text) => {
            answer.running = false;
            answer.error = Some(text.clone());
        }
    }
    Some(answered)
}

fn reopen_answer(messages: &mut Vec<ChatMessage>) -> usize {
    let last = messages.len().saturating_sub(1);
    if let Some(answer) = messages
        .last_mut()
        .filter(|message| message.speaker == Speaker::Assistant)
    {
        answer.running = true;
        return last;
    }
    let mut answer = ChatMessage::new(Speaker::Assistant, String::new());
    answer.running = true;
    messages.push(answer);
    messages.len() - 1
}

fn place_edit(messages: &mut [ChatMessage], tool: &ToolUseId, edit: &FileEdit) -> Option<usize> {
    let answered = |call: &ToolCall| call.id.as_ref() == Some(tool);
    for (index, message) in messages.iter_mut().enumerate().rev() {
        let call = message.parts.iter_mut().rev().find_map(|part| match part {
            AnswerPart::Tool(call) if answered(call) => Some(call),
            _ => None,
        });
        let Some(call) = call else {
            continue;
        };
        call.place_edit(edit.clone());
        return Some(index);
    }
    None
}

fn place_output(
    messages: &mut [ChatMessage],
    tool: &ToolUseId,
    output: &ToolOutput,
) -> Option<usize> {
    let ran = |part: &AnswerPart| matches!(part, AnswerPart::Tool(call) if call.id.as_ref() == Some(tool));
    let index = messages
        .iter()
        .rposition(|message| message.parts.iter().any(ran))?;
    messages[index]
        .tool_outputs
        .insert(tool.clone(), ShownOutput::from(output));
    Some(index)
}

fn update_tool(parts: &mut Vec<AnswerPart>, call: &ToolCall) {
    let shown = call.id.as_ref().and_then(|id| {
        parts.iter_mut().find_map(|part| match part {
            AnswerPart::Tool(shown) if shown.id.as_ref() == Some(id) => Some(shown),
            _ => None,
        })
    });
    match shown {
        Some(shown) => shown.clone_from(call),
        None => parts.push(AnswerPart::Tool(call.clone())),
    }
}

#[must_use]
pub fn last_permission_mode(entries: &[ChatEntry]) -> Option<PermissionMode> {
    entries.iter().rev().find_map(|entry| match entry {
        ChatEntry::Event(ChatEvent::PermissionMode(mode)) => Some(*mode),
        _ => None,
    })
}

#[cfg(test)]
mod tests;
