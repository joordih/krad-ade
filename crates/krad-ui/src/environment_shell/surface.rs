mod acp;
mod anchor;
mod answer;
mod answer_blocks;
mod bookmarks;
mod bus;
mod chat_menu;
mod chat_model;
mod checklist;
mod choice;
mod composer;
mod composer_rows;
mod dot_field;
mod edit_diff;
mod edit_message;
mod facts;
mod files_restore;
mod glide;
mod mentions;
mod message_actions;
mod origin;
mod outbox;
mod place;
mod prose_views;
mod shown_images;
mod status_line;
mod structured;
mod subagent_card;
mod subagents;
mod switch;
mod thread;
mod timeline;
mod tool_group;
mod tool_output;
mod transcript;
mod view;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{
    px, AppContext as _, Context, Entity, FollowMode, ListAlignment, ListState, SharedString,
    Subscription, Window,
};
use krad_domain::{
    AgentPreset, Effort, EnvironmentName, IndexEntry, Mention, ModelId, NodeField, Prompt,
    TerminalId, COMPOSER_MIN_ROWS, DEFAULT_CHAT_HARNESS,
};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::EnvironmentShell;
use anchor::Anchor;
pub(super) use chat_model::Chat;
use checklist::ChecklistView;
pub(super) use composer::EffortPopover;
use edit_message::MessageEditor;
use glide::Glide;
pub(super) use origin::age_label;
use shown_images::ShownImages;
use timeline::Timeline;
use transcript::TranscriptView;

pub(super) use acp::{AcpChoice, AcpSeed, KeptAdapterView};
pub(super) use answer::{names_its_tool, tool_question, tool_request};
pub(super) use bus::ChatPresence;
pub(super) use chat_menu::ChatCopy;
pub(super) use edit_diff::edit_diffs;
pub(super) use place::ChatPlace;
pub(super) use status_line::elapsed_label;
pub(super) use structured::restored_chats;
pub(super) use structured::StructuredChat;

const INPUT_PLACEHOLDER: &str = "Ask anything…  @ for files, / for skills";
const THINKING_LABEL: &str = "Thinking";
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

pub(super) struct ChatView {
    pub(super) chat: Chat,
    pub(super) input: Entity<TextareaState>,
    composer_rows: u16,
    _input_events: Subscription,
    pub(super) mentions: HashMap<SharedString, IndexEntry>,
    pub(super) attached: Vec<Mention>,
    pub(super) opened_thoughts: HashSet<(usize, usize)>,
    pub(super) tool_groups: HashMap<(usize, usize), bool>,
    pub(super) thread: ListState,
    prose: RefCell<prose_views::ProseViews>,
    pub(super) measured_tail: Cell<u64>,
    pub(super) text_scale: Cell<f32>,
    pub(super) pins_latest: bool,
    pub(super) runway: Cell<f32>,
    pub(super) lead: Cell<Option<f32>>,
    anchor: Anchor,
    landing: Cell<Option<f32>>,
    timeline: RefCell<Timeline>,
    glide: Option<Glide>,
    checklist_view: ChecklistView,
    editing: Option<MessageEditor>,
    pub(super) images: ShownImages,
}

impl ChatView {
    pub(super) fn takes_bus_notice(&self) -> bool {
        self.chat
            .structured()
            .is_some_and(|structured| structured.takes_bus_notice(&self.chat.outbox))
    }

    pub(super) fn queue_bus_notice(&mut self, notice: String) {
        self.chat.outbox.push(Prompt::from(notice));
    }

    fn new(input: Entity<TextareaState>, input_events: Subscription, backend: Backend) -> Self {
        Self {
            chat: Chat::new(backend),
            input,
            composer_rows: COMPOSER_MIN_ROWS,
            _input_events: input_events,
            mentions: HashMap::new(),
            attached: Vec::new(),
            opened_thoughts: HashSet::new(),
            tool_groups: HashMap::new(),
            thread: new_thread(),
            prose: RefCell::default(),
            measured_tail: Cell::new(0),
            text_scale: Cell::new(krad_domain::TextScale::DEFAULT),
            pins_latest: false,
            runway: Cell::new(0.0),
            lead: Cell::new(None),
            anchor: Anchor::Tail,
            landing: Cell::new(None),
            timeline: RefCell::default(),
            glide: None,
            checklist_view: ChecklistView::default(),
            editing: None,
            images: ShownImages::Here,
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
                .auto_grow(
                    usize::from(COMPOSER_MIN_ROWS),
                    usize::from(COMPOSER_MIN_ROWS),
                )
                .submit_on_enter(true)
                .placeholder(INPUT_PLACEHOLDER)
        });
        let events = cx.subscribe_in(
            &input,
            window,
            move |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send_on_surface(id, window, cx),
                InputEvent::Change => this.update_suggest(id, cx),
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
        let place = self.chat_place();
        let blocked = self
            .live(&place)
            .and_then(|live| live.chats.get(&id))
            .is_some_and(|chat| chat.chat.asks().blocks_typing());
        if blocked {
            return;
        }
        let Some(input) = self.composer_input(id) else {
            return;
        };
        let text = input.read(cx).value().trim().to_owned();
        let attached = self
            .live(&place)
            .and_then(|live| live.chats.get(&id))
            .is_some_and(|chat| !chat.attached.is_empty());
        if text.is_empty() && !attached {
            return;
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        let harness = self
            .live(&place)
            .and_then(|live| live.chats.get(&id))
            .map_or(DEFAULT_CHAT_HARNESS, |view| view.chat.harness());
        if harness.chats_through_gateway() {
            self.recheck_failed_gateway(cx);
        }
        let text = self.skill_message(harness, text);
        let text = match place.local().cloned() {
            Some(local) => self.with_kraddie_packs(&local, id, text),
            None => text,
        };
        if let Some(chat) = self
            .live_mut(&place)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.mentions.clear();
            let prompt = Prompt::new(&text, std::mem::take(&mut chat.attached));
            chat.chat.outbox.push(prompt);
            chat.pins_latest = true;
            chat.start_glide(Instant::now());
        }
        self.flush_outbox(&place, id, cx);
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
            self.flush_outbox(&ChatPlace::here(&env), id, cx);
        }
    }

    fn flush_outbox(&mut self, place: &ChatPlace, id: TerminalId, cx: &mut Context<Self>) {
        let structured = self
            .live(place)
            .and_then(|live| live.chats.get(&id))
            .is_some_and(|chat| chat.chat.structured().is_some());
        match (structured, place.local().cloned()) {
            (true, _) => self.flush_structured(place, id, cx),
            (false, Some(local)) => self.flush_transcript(&local, id),
            (false, None) => {}
        }
    }

    pub(super) fn show_earlier(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let place = self.chat_place();
        if self.show_earlier_in_runtime(&place, id, cx) {
            return cx.notify();
        }
        let structured = self
            .live(&self.chat_place())
            .and_then(|live| live.chats.get(&id))
            .is_some_and(|chat| chat.chat.structured().is_some());
        if !structured {
            self.show_whole_transcript(id, cx);
        }
    }

    pub(super) fn cancel_queued(&mut self, id: TerminalId, index: usize, cx: &mut Context<Self>) {
        let place = self.chat_place();
        let queued = self
            .live_mut(&place)
            .and_then(|live| live.chats.get_mut(&id))
            .and_then(|chat| chat.cancel_pending(index));
        if let Some(index) = queued {
            self.unqueue_in_runtime(&place, id, index);
        }
        cx.notify();
    }

    fn harness_change(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        command: &SurfaceCommand,
    ) -> HarnessChange {
        let SurfaceCommand::Model(harness, _) = command else {
            return HarnessChange::Kept;
        };
        let Some(live) = self.live_mut(place) else {
            return HarnessChange::Kept;
        };
        let Some(Chat {
            backend: Backend::Structured(structured),
            messages,
            ..
        }) = live.chats.get_mut(&id).map(|view| &mut view.chat)
        else {
            return HarnessChange::Kept;
        };
        if *harness == structured.harness {
            return HarnessChange::Kept;
        }
        let Some(local) = place.local().cloned() else {
            return HarnessChange::Refused;
        };
        if !messages.is_empty() || !structured.is_fresh() {
            return HarnessChange::Refused;
        }
        self.leave_runtime(place, id);
        let Some(Backend::Structured(structured)) = self
            .live_mut(place)
            .and_then(|live| live.chats.get_mut(&id))
            .map(|view| &mut view.chat.backend)
        else {
            return HarnessChange::Kept;
        };
        structured.switch_harness(*harness);
        self.set_node(&local, id, NodeField::Harness(Some(*harness)));
        self.seed_acp_chats();
        HarnessChange::Switched
    }

    pub(super) fn command_surface(
        &mut self,
        id: TerminalId,
        command: SurfaceCommand,
        cx: &mut Context<Self>,
    ) {
        if let SurfaceCommand::Effort(effort) = command {
            if self.choose_acp_effort(id, effort, cx) {
                return;
            }
        }
        let place = self.chat_place();
        match self.harness_change(&place, id, &command) {
            HarnessChange::Refused => return,
            HarnessChange::Switched => self.store(),
            HarnessChange::Kept => {}
        }
        let chosen = command.clone();
        self.run_surface_command(&place, id, command);
        if let Some(local) = place.local().cloned() {
            if matches!(chosen, SurfaceCommand::Model(..)) {
                self.fit_effort_to_model(&local, id, cx);
            }
            self.remember_choice(&local, id, &chosen, cx);
        }
        cx.notify();
    }

    pub(super) fn stop_turn_in(&mut self, place: &ChatPlace, id: TerminalId) {
        self.run_surface_command(place, id, SurfaceCommand::Stop);
    }

    fn run_surface_command(&mut self, place: &ChatPlace, id: TerminalId, command: SurfaceCommand) {
        self.command_runtime_chat(place, id, &command);
        match self
            .live_mut(place)
            .and_then(|live| live.chats.get_mut(&id))
            .map(|chat| &mut chat.chat.backend)
        {
            Some(Backend::Structured(structured)) => structured.command(command),
            Some(Backend::Transcript(_)) | None => {
                if let Some(local) = place.local().cloned() {
                    self.command_transcript(&local, id, &command);
                }
            }
        }
    }
}
