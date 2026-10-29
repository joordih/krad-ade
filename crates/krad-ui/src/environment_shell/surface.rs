mod answer;
mod composer;
mod dot_field;
mod edit_diff;
mod facts;
mod outbox;
mod structured;
mod subagent_card;
mod subagents;
mod switch;
mod thread;
mod tool_group;
mod transcript;
mod view;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{
    px, AppContext as _, Context, Entity, FollowMode, ListAlignment, ListState, SharedString,
    Subscription, Window,
};
use krad_domain::{
    is_worker_brief, AgentPreset, ChatEntry, Effort, EnvironmentName, ModelId, TerminalId,
};
use krad_repo::IndexEntry;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime};

use super::ask::AskQueue;
use super::chat::{ChatMessage, Composer};
use super::EnvironmentShell;
use outbox::Outbox;
use subagents::Subagents;
use transcript::TranscriptView;

pub(super) use answer::tool_icon;
pub(super) use edit_diff::edit_diff;
pub(super) use facts::AccountUsage;
pub(super) use structured::restored_chats;
pub(super) use structured::StructuredChat;

const INPUT_ROWS: usize = 2;
const INPUT_PLACEHOLDER: &str = "Ask anything…  @ for files, / for skills";
const SURFACE_HARNESS: AgentPreset = AgentPreset::ClaudeCode;
const THINKING_LABEL: &str = "Thinking";
const WORKING_LABEL: &str = "Working";
const MINUTE: Duration = Duration::from_mins(1);
const HOUR: Duration = Duration::from_hours(1);
const DAY: Duration = Duration::from_hours(24);
const JUST_NOW: &str = "just now";
const MINUTES_UNIT: &str = "min";
const HOURS_UNIT: &str = "h";
const DAYS_UNIT: &str = "d";
const AGO: &str = "ago";
const THREAD_OVERDRAW: f32 = 800.0;

fn new_thread() -> ListState {
    let thread = ListState::new(0, ListAlignment::Top, px(THREAD_OVERDRAW));
    thread.set_follow_mode(FollowMode::Tail);
    thread
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SurfaceCommand {
    Stop,
    Model(ModelId),
    Effort(Effort),
    ToggleThinking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Opener {
    Lead,
    User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Origin {
    pub(super) opener: Opener,
    pub(super) started: Option<SystemTime>,
}

impl Origin {
    fn of(entries: &[ChatEntry], started: Option<SystemTime>) -> Option<Self> {
        let first = entries.iter().find_map(|entry| match entry {
            ChatEntry::User { text, .. } => Some(text),
            ChatEntry::Event(_) | ChatEntry::Asked(_) => None,
        })?;
        let opener = if is_worker_brief(first) {
            Opener::Lead
        } else {
            Opener::User
        };
        Some(Self { opener, started })
    }
}

pub(super) enum Backend {
    Transcript(Box<TranscriptView>),
    Structured(Box<StructuredChat>),
}

pub(super) struct SurfaceChat {
    pub(super) messages: Vec<ChatMessage>,
    pub(super) origin: Option<Origin>,
    pub(super) input: Entity<TextareaState>,
    _input_events: Subscription,
    pub(super) mentions: HashMap<SharedString, IndexEntry>,
    pub(super) opened_thoughts: HashSet<(usize, usize)>,
    pub(super) tool_groups: HashMap<(usize, usize), bool>,
    pub(super) thread: ListState,
    pub(super) measured_tail: Cell<u64>,
    pub(super) working_since: Cell<Option<Instant>>,
    outbox: Outbox,
    pub(super) problem: Option<String>,
    pub(super) backend: Backend,
    subagents: Subagents,
}

impl SurfaceChat {
    pub(super) fn takes_bus_notice(&self) -> bool {
        self.structured()
            .is_some_and(|structured| structured.takes_bus_notice(&self.outbox))
    }

    pub(super) fn queue_bus_notice(&mut self, notice: String) {
        self.outbox.push(notice);
    }

    fn new(input: Entity<TextareaState>, input_events: Subscription, backend: Backend) -> Self {
        Self {
            messages: Vec::new(),
            origin: None,
            input,
            _input_events: input_events,
            mentions: HashMap::new(),
            opened_thoughts: HashSet::new(),
            tool_groups: HashMap::new(),
            thread: new_thread(),
            measured_tail: Cell::new(0),
            working_since: Cell::new(None),
            outbox: Outbox::default(),
            problem: None,
            backend,
            subagents: Subagents::default(),
        }
    }

    pub(super) const fn structured(&self) -> Option<&StructuredChat> {
        match &self.backend {
            Backend::Structured(structured) => Some(structured),
            Backend::Transcript(_) => None,
        }
    }

    pub(super) const fn asks(&self) -> &AskQueue {
        match &self.backend {
            Backend::Structured(structured) => &structured.asks,
            Backend::Transcript(view) => &view.asks,
        }
    }

    pub(super) fn asks_mut(&mut self) -> &mut AskQueue {
        match &mut self.backend {
            Backend::Structured(structured) => &mut structured.asks,
            Backend::Transcript(view) => &mut view.asks,
        }
    }

    pub(super) fn structured_mut(&mut self) -> Option<&mut StructuredChat> {
        match &mut self.backend {
            Backend::Structured(structured) => Some(structured),
            Backend::Transcript(_) => None,
        }
    }
}

pub(super) fn age_label(elapsed: Duration) -> String {
    if elapsed < MINUTE {
        return JUST_NOW.to_owned();
    }
    let (unit, name) = if elapsed < HOUR {
        (MINUTE, MINUTES_UNIT)
    } else if elapsed < DAY {
        (HOUR, HOURS_UNIT)
    } else {
        (DAY, DAYS_UNIT)
    };
    format!("{} {name} {AGO}", elapsed.as_secs() / unit.as_secs())
}

impl EnvironmentShell {
    fn chat_input(
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<TextareaState>, Subscription) {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(INPUT_ROWS)
                .submit_on_enter(true)
                .placeholder(INPUT_PLACEHOLDER)
        });
        let events = cx.subscribe_in(
            &input,
            window,
            move |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send_on_surface(id, window, cx),
                InputEvent::Change => this.update_suggest(Composer::Surface(id), cx),
                InputEvent::PressEnter { .. } | InputEvent::Focus | InputEvent::Blur => {}
            },
        );
        (input, events)
    }

    pub(super) fn send_on_surface(
        &mut self,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(input) = self.composer_input(Composer::Surface(id)) else {
            return;
        };
        let text = input.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        let text = self.shared_skill_message(&text).unwrap_or(text);
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.mentions.clear();
            chat.outbox.push(text);
            chat.thread.scroll_to_end();
        }
        self.flush_outbox(&env, id, cx);
        cx.notify();
    }

    pub(super) fn flush_noticed_chats(&mut self, cx: &mut Context<Self>) {
        let noticed: Vec<(EnvironmentName, TerminalId)> = self
            .envs
            .iter_mut()
            .flat_map(|(env, live)| {
                std::mem::take(&mut live.noticed_chats)
                    .into_iter()
                    .map(move |id| (env.clone(), id))
            })
            .collect();
        for (env, id) in noticed {
            self.flush_outbox(&env, id, cx);
        }
    }

    fn flush_outbox(&mut self, env: &EnvironmentName, id: TerminalId, cx: &mut Context<Self>) {
        let structured = self
            .envs
            .get(env)
            .and_then(|live| live.chats.get(&id))
            .is_some_and(|chat| chat.structured().is_some());
        if structured {
            self.flush_structured(env, id, cx);
        } else {
            self.flush_transcript(env, id);
        }
    }

    pub(super) fn show_earlier(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let structured = self
            .envs
            .get(&self.active)
            .and_then(|live| live.chats.get(&id))
            .is_some_and(|chat| chat.structured().is_some());
        if structured {
            self.show_earlier_structured(id, cx);
        } else {
            self.show_whole_transcript(id, cx);
        }
    }

    pub(super) fn cancel_queued(&mut self, id: TerminalId, index: usize, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.outbox.cancel(index);
        }
        cx.notify();
    }

    pub(super) fn command_surface(
        &mut self,
        id: TerminalId,
        command: SurfaceCommand,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        match self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
            .map(|chat| &mut chat.backend)
        {
            Some(Backend::Structured(structured)) => structured.command(command),
            Some(Backend::Transcript(_)) | None => self.command_transcript(&env, id, &command),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ChatEvent;

    #[test]
    fn ages_read_in_the_largest_whole_unit() {
        assert_eq!(age_label(Duration::from_secs(20)), "just now");
        assert_eq!(age_label(6 * MINUTE + Duration::from_secs(30)), "6 min ago");
        assert_eq!(age_label(3 * HOUR), "3 h ago");
        assert_eq!(age_label(2 * DAY), "2 d ago");
    }

    #[test]
    fn a_conversation_a_worker_brief_opened_comes_from_the_lead() {
        let user = |text: String| ChatEntry::User {
            text,
            preset: AgentPreset::ClaudeCode,
            model: None,
        };
        let brief = krad_domain::chat_worker_brief("Verify the signature");
        let origin = Origin::of(&[ChatEntry::Event(ChatEvent::Done), user(brief)], None);
        assert_eq!(origin.map(|origin| origin.opener), Some(Opener::Lead));
        let asked = Origin::of(&[user("Why?".to_owned())], None);
        assert_eq!(asked.map(|origin| origin.opener), Some(Opener::User));
        assert_eq!(Origin::of(&[], None), None);
    }
}
