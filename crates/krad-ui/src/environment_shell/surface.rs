mod answer;
mod checklist;
mod composer;
mod dot_field;
mod edit_diff;
mod edit_message;
mod facts;
mod glide;
mod mentions;
mod message_actions;
mod origin;
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
use krad_domain::{AgentPreset, Effort, EnvironmentName, Mention, ModelId, Prompt, TerminalId};
use krad_repo::IndexEntry;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::ask::AskQueue;
use super::chat::{ChatMessage, Composer};
use super::EnvironmentShell;
use checklist::ChecklistView;
use edit_message::MessageEditor;
use glide::Glide;
use origin::Origin;
use outbox::Outbox;
use subagents::Subagents;
use transcript::{TranscriptView, TRANSCRIPT_HARNESS};

pub(super) use answer::tool_question;
pub(super) use edit_diff::edit_diff;
pub(super) use facts::AccountUsage;
pub(super) use structured::restored_chats;
pub(super) use structured::StructuredChat;
pub(super) use switch::reopens_as_chat;

const INPUT_ROWS: usize = 2;
const INPUT_PLACEHOLDER: &str = "Ask anything…  @ for files, / for skills";
const THINKING_LABEL: &str = "Thinking";
const WORKING_LABEL: &str = "Working";
const THREAD_OVERDRAW: f32 = 800.0;

fn new_thread() -> ListState {
    let thread = ListState::new(0, ListAlignment::Top, px(THREAD_OVERDRAW));
    thread.set_follow_mode(FollowMode::Tail);
    thread
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SurfaceCommand {
    Stop,
    Model(AgentPreset, ModelId),
    Effort(Effort),
    ToggleThinking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HarnessChange {
    Kept,
    Switched,
    Refused,
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
    pub(super) attached: Vec<Mention>,
    pub(super) opened_thoughts: HashSet<(usize, usize)>,
    pub(super) tool_groups: HashMap<(usize, usize), bool>,
    pub(super) thread: ListState,
    pub(super) measured_tail: Cell<u64>,
    pub(super) pins_latest: bool,
    pub(super) runway: Cell<f32>,
    pub(super) lead: Cell<Option<f32>>,
    glide: Option<Glide>,
    pub(super) working_since: Cell<Option<Instant>>,
    outbox: Outbox,
    pub(super) problem: Option<String>,
    pub(super) backend: Backend,
    subagents: Subagents,
    checklist_view: ChecklistView,
    editing: Option<MessageEditor>,
}

impl SurfaceChat {
    pub(super) fn takes_bus_notice(&self) -> bool {
        self.structured()
            .is_some_and(|structured| structured.takes_bus_notice(&self.outbox))
    }

    pub(super) fn queue_bus_notice(&mut self, notice: String) {
        self.outbox.push(Prompt::from(notice));
    }

    fn new(input: Entity<TextareaState>, input_events: Subscription, backend: Backend) -> Self {
        Self {
            messages: Vec::new(),
            origin: None,
            input,
            _input_events: input_events,
            mentions: HashMap::new(),
            attached: Vec::new(),
            opened_thoughts: HashSet::new(),
            tool_groups: HashMap::new(),
            thread: new_thread(),
            measured_tail: Cell::new(0),
            pins_latest: false,
            runway: Cell::new(0.0),
            lead: Cell::new(None),
            glide: None,
            working_since: Cell::new(None),
            outbox: Outbox::default(),
            problem: None,
            backend,
            subagents: Subagents::default(),
            checklist_view: ChecklistView::default(),
            editing: None,
        }
    }

    pub(super) const fn harness(&self) -> AgentPreset {
        match &self.backend {
            Backend::Structured(structured) => structured.harness,
            Backend::Transcript(_) => TRANSCRIPT_HARNESS,
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
        let blocked = self
            .envs
            .get(&env)
            .and_then(|live| live.chats.get(&id))
            .is_some_and(|chat| chat.asks().blocks_typing());
        if blocked {
            return;
        }
        let Some(input) = self.composer_input(Composer::Surface(id)) else {
            return;
        };
        let text = input.read(cx).value().trim().to_owned();
        let attached = self
            .envs
            .get(&env)
            .and_then(|live| live.chats.get(&id))
            .is_some_and(|chat| !chat.attached.is_empty());
        if text.is_empty() && !attached {
            return;
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        let harness = self
            .envs
            .get(&env)
            .and_then(|live| live.chats.get(&id))
            .map_or(AgentPreset::ClaudeCode, SurfaceChat::harness);
        if harness == AgentPreset::Gateway {
            self.recheck_failed_gateway(cx);
        }
        let text = if harness == AgentPreset::ClaudeCode {
            self.shared_skill_message(&text).unwrap_or(text)
        } else {
            text
        };
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.mentions.clear();
            let prompt = Prompt::new(&text, std::mem::take(&mut chat.attached));
            chat.outbox.push(prompt);
            chat.pins_latest = true;
            chat.start_glide(Instant::now());
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

    fn harness_change(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        command: &SurfaceCommand,
    ) -> HarnessChange {
        let SurfaceCommand::Model(harness, _) = command else {
            return HarnessChange::Kept;
        };
        let Some(live) = self.envs.get_mut(env) else {
            return HarnessChange::Kept;
        };
        let Some(SurfaceChat {
            backend: Backend::Structured(structured),
            messages,
            ..
        }) = live.chats.get_mut(&id)
        else {
            return HarnessChange::Kept;
        };
        if *harness == structured.harness {
            return HarnessChange::Kept;
        }
        if !messages.is_empty() || !structured.is_fresh() {
            return HarnessChange::Refused;
        }
        structured.switch_harness(*harness);
        if let Some(node) = live.data.nodes.iter_mut().find(|node| node.id == id) {
            node.harness = Some(*harness);
        }
        HarnessChange::Switched
    }

    pub(super) fn command_surface(
        &mut self,
        id: TerminalId,
        command: SurfaceCommand,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        match self.harness_change(&env, id, &command) {
            HarnessChange::Refused => return,
            HarnessChange::Switched => self.store(),
            HarnessChange::Kept => {}
        }
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
