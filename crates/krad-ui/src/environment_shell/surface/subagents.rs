use gpui_kit::{App, Context, ScrollHandle, Task};
use krad_client::ClientError;
use krad_domain::{
    AgentPreset, ChatEntry, ChatEvent, EnvironmentName, SubagentMeta, TerminalId, ToolKind,
    ToolUseId,
};
use krad_protocol::{
    Answer, ChatCommand, Command, KnownRecord, SubagentsAsk, SubagentsFrom, SubagentsRead,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use super::super::chat::{fold_entry, AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use super::anchor::follows_tail;
use super::view::at_bottom;
use super::{Chat, ChatView};

const SUBAGENT_POLL: Duration = Duration::from_secs(1);

pub(super) struct SubagentRun {
    pub(super) meta: SubagentMeta,
    pub(super) messages: Vec<ChatMessage>,
    record: PathBuf,
    read: u64,
}

impl SubagentRun {
    fn take(&mut self, entries: &[ChatEntry]) {
        for entry in entries {
            if self.is_own_call(entry) {
                continue;
            }
            if self.messages.is_empty() && !matches!(entry, ChatEntry::User { .. }) {
                fold_entry(
                    &mut self.messages,
                    &ChatEntry::Event(ChatEvent::TurnStarted),
                );
            }
            fold_entry(&mut self.messages, entry);
        }
    }

    fn is_own_call(&self, entry: &ChatEntry) -> bool {
        matches!(
            entry,
            ChatEntry::Event(ChatEvent::Tool(call))
                if call.id.as_ref() == Some(&self.meta.tool_use_id)
        )
    }

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

enum Look {
    Gone,
    Idle,
    Read(SubagentsAsk),
}

fn agent_calls(messages: &[ChatMessage]) -> impl Iterator<Item = &ToolUseId> {
    messages
        .iter()
        .flat_map(|message| &message.parts)
        .filter_map(|part| match part {
            AnswerPart::Tool(call) if call.kind == ToolKind::Subagent => call.id.as_ref(),
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
                let Ok(look) = this.update(cx, |shell, cx| shell.subagent_look(&env, id, cx))
                else {
                    break;
                };
                let ask = match look {
                    Look::Gone => break,
                    Look::Idle => continue,
                    Look::Read(ask) => ask,
                };
                let command = Command::Chat(ChatCommand::Subagents {
                    environment: env.clone(),
                    ask,
                });
                let Ok(asked) = this.update(cx, |shell, cx| shell.runtime.ask(command, cx)) else {
                    break;
                };
                let read = match asked.await {
                    Ok(Answer::Subagents(read)) => *read,
                    Err(ClientError::Stopped) => break,
                    Ok(_) | Err(_) => continue,
                };
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
            chat.chat.subagents.watch = Some(watch);
        }
    }

    fn subagent_look(&self, env: &EnvironmentName, id: TerminalId, cx: &App) -> Look {
        let Some(live) = self.envs.get(env) else {
            return Look::Gone;
        };
        let Some(chat) = live.chats.get(&id) else {
            return Look::Gone;
        };
        let subagents = &chat.chat.subagents;
        let following = agent_calls(&chat.chat.messages).any(|call| {
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
        let from = match chat.chat.harness() {
            AgentPreset::Gateway => SubagentsFrom::Gateway {
                cwd: self.chat_folder(env, id, cx),
            },
            AgentPreset::ClaudeCode => SubagentsFrom::Claude {
                root: live.data.group_of(id).map(|group| group.root.clone()),
                home: subagents.home.clone(),
                conversation: subagents.conversation.clone(),
            },
            AgentPreset::Codex
            | AgentPreset::Opencode
            | AgentPreset::Shell
            | AgentPreset::Acp(_) => return Look::Idle,
        };
        Look::Read(SubagentsAsk {
            resume,
            from,
            known: subagents
                .runs
                .values()
                .map(|run| KnownRecord {
                    record: run.record.clone(),
                    read: run.read,
                    finished: run.meta.finished,
                })
                .collect(),
        })
    }

    fn apply_subagents(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        read: SubagentsRead,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        else {
            return;
        };
        let ChatView {
            chat: Chat { subagents, .. },
            thread,
            anchor,
            ..
        } = chat;
        subagents.home = read.home;
        subagents.conversation = read.conversation;
        if read.updates.is_empty() {
            return;
        }
        let in_main = subagents.trail.is_empty();
        let followed = if in_main {
            follows_tail(*anchor, thread)
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
            run.meta = update.meta;
            run.take(&update.chunk.entries);
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
        step(&mut chat.chat.subagents.trail);
        chat.opened_thoughts.clear();
        chat.thread.remeasure();
        chat.chat.subagents.scroll.scroll_to_bottom();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{Speaker, ToolCall};

    fn agent_call(id: &str, name: &str) -> ChatEntry {
        ChatEntry::Event(ChatEvent::Tool(ToolCall {
            id: Some(ToolUseId::from(id)),
            name: name.to_owned(),
            kind: ToolKind::of_claude_code(name),
            summary: String::new(),
            edits: Vec::new(),
        }))
    }

    #[test]
    fn a_fork_without_a_prompt_of_its_own_shows_its_work_but_not_the_call_that_forked_it() {
        let mut run = SubagentRun {
            meta: SubagentMeta {
                tool_use_id: ToolUseId::from("fork"),
                agent_type: Some("fork".to_owned()),
                description: Some("UI core".to_owned()),
                finished: false,
            },
            messages: Vec::new(),
            record: PathBuf::new(),
            read: 0,
        };
        run.take(&[
            agent_call("fork", "Agent"),
            ChatEntry::Event(ChatEvent::Text("Task: build the UI core.".to_owned())),
            agent_call("t1", "Bash"),
        ]);
        assert_eq!(run.messages.len(), 1);
        assert_eq!(run.messages[0].speaker, Speaker::Assistant);
        assert_eq!(run.messages[0].text, "Task: build the UI core.");
        assert_eq!(run.tool_count(), 1);
    }

    fn answer_with(calls: &[(&str, &str)]) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        fold_entry(
            &mut messages,
            &ChatEntry::User {
                text: "Map it".to_owned(),
                preset: AgentPreset::ClaudeCode,
                model: None,
                images: Vec::new(),
            },
        );
        for (id, name) in calls {
            fold_entry(
                &mut messages,
                &ChatEntry::Event(ChatEvent::Tool(ToolCall {
                    id: Some(ToolUseId::from(*id)),
                    name: (*name).to_owned(),
                    kind: ToolKind::of_claude_code(name),
                    summary: String::new(),
                    edits: Vec::new(),
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
