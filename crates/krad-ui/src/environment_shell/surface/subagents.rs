use gpui_kit::{Context, ScrollHandle, Task};
use krad_chat::{
    claude_conversation_file, claude_subagents, read_subagent_meta, read_subagent_record,
    SubagentMeta, TranscriptChunk,
};
use krad_domain::{EnvironmentName, ResumeId, TerminalId, ToolUseId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use super::super::chat::{fold_entry, workplace_home, AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use super::answer::is_agent_tool;
use super::view::{at_bottom, following};
use super::SurfaceChat;

const SUBAGENT_POLL: Duration = Duration::from_secs(1);

pub(super) struct SubagentRun {
    pub(super) meta: SubagentMeta,
    pub(super) messages: Vec<ChatMessage>,
    record: PathBuf,
    read: u64,
}

impl SubagentRun {
    pub(super) fn tool_count(&self) -> usize {
        self.messages
            .iter()
            .flat_map(|message| &message.parts)
            .filter(|part| matches!(part, AnswerPart::Tool(_)))
            .count()
    }
}

#[derive(Default)]
pub(super) struct Subagents {
    pub(super) runs: HashMap<ToolUseId, SubagentRun>,
    pub(super) trail: Vec<ToolUseId>,
    pub(super) scroll: ScrollHandle,
    home: Option<PathBuf>,
    conversation: Option<PathBuf>,
    watch: Option<Task<()>>,
}

struct SubagentJob {
    resume: ResumeId,
    root: Option<PathBuf>,
    home: Option<PathBuf>,
    conversation: Option<PathBuf>,
    known: Vec<(PathBuf, u64, bool)>,
}

struct SubagentUpdate {
    meta: SubagentMeta,
    record: PathBuf,
    chunk: TranscriptChunk,
}

struct SubagentRead {
    home: Option<PathBuf>,
    conversation: Option<PathBuf>,
    updates: Vec<SubagentUpdate>,
}

enum Look {
    Gone,
    Idle,
    Read(SubagentJob),
}

impl SubagentJob {
    fn run(self) -> SubagentRead {
        let home = self
            .home
            .or_else(|| workplace_home(self.root.as_deref()).ok().flatten());
        let conversation = self.conversation.or_else(|| {
            home.as_deref()
                .and_then(|home| claude_conversation_file(home, &self.resume))
        });
        let updates = conversation.as_deref().map_or_default(|conversation| {
            claude_subagents(conversation)
                .into_iter()
                .filter_map(|files| {
                    let (from, done) = self
                        .known
                        .iter()
                        .find(|(record, _, _)| *record == files.record)
                        .map_or((0, false), |(_, read, done)| (*read, *done));
                    if done {
                        return None;
                    }
                    let meta = read_subagent_meta(&files.meta).ok()?;
                    let chunk = read_subagent_record(&files.record, from).ok()?;
                    Some(SubagentUpdate {
                        meta,
                        record: files.record,
                        chunk,
                    })
                })
                .collect()
        });
        SubagentRead {
            home,
            conversation,
            updates,
        }
    }
}

fn agent_calls(messages: &[ChatMessage]) -> impl Iterator<Item = &ToolUseId> {
    messages
        .iter()
        .flat_map(|message| &message.parts)
        .filter_map(|part| match part {
            AnswerPart::Tool(call) if is_agent_tool(&call.name) => call.id.as_ref(),
            _ => None,
        })
}

impl EnvironmentShell {
    pub(super) fn watch_subagents(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let watch = cx.spawn({
            let env = env.clone();
            async move |this, cx| loop {
                cx.background_executor().timer(SUBAGENT_POLL).await;
                let Ok(look) = this.update(cx, |shell, _| shell.subagent_look(&env, id)) else {
                    break;
                };
                let job = match look {
                    Look::Gone => break,
                    Look::Idle => continue,
                    Look::Read(job) => job,
                };
                let read = cx
                    .background_executor()
                    .spawn(async move { job.run() })
                    .await;
                if this
                    .update(cx, |shell, cx| shell.apply_subagents(&env, id, read, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        if let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.subagents.watch = Some(watch);
        }
    }

    fn subagent_look(&self, env: &EnvironmentName, id: TerminalId) -> Look {
        let Some(live) = self.envs.get(env) else {
            return Look::Gone;
        };
        let Some(chat) = live.chats.get(&id) else {
            return Look::Gone;
        };
        let subagents = &chat.subagents;
        let following = agent_calls(&chat.messages).any(|call| {
            subagents
                .runs
                .get(call)
                .is_none_or(|run| !run.meta.finished)
        });
        let resume = live
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.resume.clone());
        let (true, Some(resume)) = (following, resume) else {
            return Look::Idle;
        };
        Look::Read(SubagentJob {
            resume,
            root: live.data.group_of(id).map(|group| group.root.clone()),
            home: subagents.home.clone(),
            conversation: subagents.conversation.clone(),
            known: subagents
                .runs
                .values()
                .map(|run| (run.record.clone(), run.read, run.meta.finished))
                .collect(),
        })
    }

    fn apply_subagents(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        read: SubagentRead,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        else {
            return;
        };
        let SurfaceChat {
            subagents, thread, ..
        } = chat;
        subagents.home = read.home;
        subagents.conversation = read.conversation;
        if read.updates.is_empty() {
            return;
        }
        let in_main = subagents.trail.is_empty();
        let followed = if in_main {
            following(thread)
        } else {
            at_bottom(&subagents.scroll)
        };
        for update in read.updates {
            let run = subagents
                .runs
                .entry(update.meta.tool_use_id.clone())
                .or_insert_with(|| SubagentRun {
                    meta: update.meta.clone(),
                    messages: Vec::new(),
                    record: update.record.clone(),
                    read: 0,
                });
            for entry in &update.chunk.entries {
                fold_entry(&mut run.messages, entry);
            }
            run.meta = update.meta;
            run.read = update.chunk.next;
        }
        thread.remeasure();
        match (followed, in_main) {
            (true, true) => thread.scroll_to_end(),
            (true, false) => subagents.scroll.scroll_to_bottom(),
            (false, _) => {}
        }
        cx.notify();
    }

    pub(super) fn enter_subagent(
        &mut self,
        id: TerminalId,
        call: &ToolUseId,
        cx: &mut Context<Self>,
    ) {
        self.walk_subagents(id, |trail| trail.push(call.clone()), cx);
    }

    pub(super) fn leave_subagent(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        self.walk_subagents(
            id,
            |trail| {
                trail.pop();
            },
            cx,
        );
    }

    fn walk_subagents(
        &mut self,
        id: TerminalId,
        step: impl FnOnce(&mut Vec<ToolUseId>),
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        else {
            return;
        };
        step(&mut chat.subagents.trail);
        chat.opened_thoughts.clear();
        chat.thread.remeasure();
        chat.subagents.scroll.scroll_to_bottom();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_chat::{ChatEvent, Speaker, ToolCall};
    use krad_domain::{AgentPreset, ChatEntry};

    fn answer_with(calls: &[(&str, &str)]) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        fold_entry(
            &mut messages,
            &ChatEntry::User {
                text: "Map it".to_owned(),
                preset: AgentPreset::ClaudeCode,
                model: None,
            },
        );
        for (id, name) in calls {
            fold_entry(
                &mut messages,
                &ChatEntry::Event(ChatEvent::Tool(ToolCall {
                    id: Some(ToolUseId::from(*id)),
                    name: (*name).to_owned(),
                    summary: String::new(),
                    edit: None,
                })),
            );
        }
        messages
    }

    #[test]
    fn only_agent_calls_start_subagents() {
        let messages = answer_with(&[("t1", "Read"), ("t2", "Agent"), ("t3", "Task")]);
        assert_eq!(messages[1].speaker, Speaker::Assistant);
        let calls: Vec<&str> = agent_calls(&messages).map(ToolUseId::as_str).collect();
        assert_eq!(calls, ["t2", "t3"]);
    }
}
