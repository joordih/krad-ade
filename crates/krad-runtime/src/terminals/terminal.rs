use krad_domain::{
    classify_agent_state, AgentPreset, AgentState, ProcessRun, TerminalMeta,
    SCROLLBACK_PERSIST_LINES,
};
use krad_protocol::{TerminalScroll, WriteBy};
use krad_session::{AnsiPalette, GridSize, LiveSession, PtyInput, SharedKiller, WheelInput};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::action::TerminalAction;
use super::launch::OutputTap;
use super::view::{Shared, ShownImage, TerminalFrame, TerminalStatus};

const SCROLLBACK_CLEARED: &[u8] = b"\x1b[3J";
const SCROLLBACK_REFRESH: Duration = Duration::from_secs(5);
const WRITER_LOST: &str = "writer lost";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Generation(pub(crate) u64);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scrollback {
    Saved,
    Stale,
    Due,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Written {
    Sent,
    Lost,
}

pub(crate) struct Terminal {
    pub(crate) generation: Generation,
    session: LiveSession,
    harness: Option<AgentPreset>,
    hooked: bool,
    classified: Option<AgentState>,
    shared: Arc<Shared>,
    published: TerminalStatus,
    dirty: bool,
    scrollback: Scrollback,
    scrollback_read: Instant,
    images: HashMap<u32, ShownImage>,
    tap: Option<OutputTap>,
}

pub(crate) struct Opened {
    pub(crate) generation: Generation,
    pub(crate) session: LiveSession,
    pub(crate) harness: Option<AgentPreset>,
    pub(crate) shared: Arc<Shared>,
    pub(crate) tap: Option<OutputTap>,
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

impl Terminal {
    pub(crate) fn new(opened: Opened) -> Self {
        let mut terminal = Self {
            generation: opened.generation,
            session: opened.session,
            harness: opened.harness,
            hooked: false,
            classified: None,
            shared: opened.shared,
            published: TerminalStatus::default(),
            dirty: true,
            scrollback: Scrollback::Saved,
            scrollback_read: Instant::now(),
            images: HashMap::new(),
            tap: opened.tap,
        };
        terminal.publish_status();
        terminal
    }

    pub(crate) const fn run(&self) -> &ProcessRun {
        &self.session.state
    }

    pub(crate) const fn is_hooked(&self) -> bool {
        self.hooked
    }

    pub(crate) fn bracketed_paste(&self) -> bool {
        self.session.engine.bracketed_paste()
    }

    pub(crate) fn recent_lines(&mut self, lines: usize) -> Vec<String> {
        self.session.engine.recent_lines(lines)
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Option<AgentState> {
        self.tap_output(bytes);
        self.session.feed(bytes);
        self.dirty = true;
        self.went_stale();
        if self.harness.is_some_and(AgentPreset::reopens_as_chat)
            && contains(bytes, SCROLLBACK_CLEARED)
        {
            self.hooked = true;
        }
        self.publish_status();
        if self.hooked {
            return None;
        }
        let bell = self.session.engine.take_bell();
        let running = self.session.state.is_running();
        self.classify(classify_agent_state(
            0.0,
            running,
            bell,
            self.harness.is_some(),
        ))
    }

    fn tap_output(&mut self, bytes: &[u8]) {
        let Some(tap) = &mut self.tap else {
            return;
        };
        if let Err(error) = tap.try_send(bytes.to_vec()) {
            if error.is_disconnected() {
                self.tap = None;
            } else {
                tracing::trace!("an output tap is full; it misses a chunk");
            }
        }
    }

    pub(crate) fn reclassify(&mut self, now: Instant) -> Option<AgentState> {
        if !self.session.state.is_running() || self.hooked {
            return None;
        }
        let idle = now
            .saturating_duration_since(self.session.last_output)
            .as_secs_f32();
        let bell = self.session.engine.take_bell();
        self.classify(classify_agent_state(
            idle,
            true,
            bell,
            self.harness.is_some(),
        ))
    }

    fn classify(&mut self, state: AgentState) -> Option<AgentState> {
        (self.classified != Some(state)).then(|| {
            self.classified = Some(state);
            state
        })
    }

    pub(crate) fn ready(&mut self, cwd: Option<String>, killer: SharedKiller) {
        self.session.killer = Some(killer);
        self.session.cwd = cwd;
        self.went_stale();
    }

    fn went_stale(&mut self) {
        if self.scrollback == Scrollback::Saved {
            self.scrollback = Scrollback::Stale;
        }
    }

    pub(crate) fn exit(&mut self, code: Option<i32>) {
        self.session.state.exit(code);
        self.ended();
    }

    pub(crate) fn fail(&mut self, error: String) {
        self.session.state = ProcessRun::Failed { error };
        self.ended();
    }

    fn ended(&mut self) {
        self.dirty = true;
        self.scrollback = Scrollback::Due;
        self.publish_status();
    }

    pub(crate) fn write(&mut self, bytes: Vec<u8>, by: WriteBy) -> Written {
        match by {
            WriteBy::User => {
                self.session.engine.scroll_to_bottom();
                self.session.first_input.user_typed();
                self.dirty = true;
            }
            WriteBy::App => {}
            WriteBy::AppOnUntouchedPrompt => {
                if !self.session.first_input.claim_for_app() {
                    tracing::debug!("app input skipped: the terminal was already in use");
                    return Written::Sent;
                }
            }
        }
        if self.session.writer_tx.send(PtyInput::Bytes(bytes)).is_ok() {
            return Written::Sent;
        }
        if by == WriteBy::User {
            self.fail(WRITER_LOST.to_owned());
        }
        Written::Lost
    }

    pub(crate) fn resize(&mut self, size: GridSize) {
        self.session.resize_grid(size);
        self.dirty = true;
    }

    pub(crate) fn scroll(&mut self, scroll: TerminalScroll) {
        match scroll {
            TerminalScroll::Lines { lines } => self.session.engine.scroll_lines(lines),
            TerminalScroll::Bottom => self.session.engine.scroll_to_bottom(),
            TerminalScroll::Unknown => return,
        }
        self.dirty = true;
    }

    pub(crate) fn act(&mut self, action: TerminalAction) -> Written {
        let engine = &mut self.session.engine;
        match action {
            TerminalAction::Key { key, text, held } => {
                if let Some(bytes) = engine.encode_key(&key, text.as_deref(), held) {
                    return self.write(bytes, WriteBy::User);
                }
            }
            TerminalAction::Wheel { lines, at } => match engine.wheel(lines, at) {
                WheelInput::History(lines) => engine.scroll_lines(lines),
                WheelInput::Program(bytes) => return self.write(bytes, WriteBy::App),
            },
            TerminalAction::Press { at, report: true } => {
                if let Some(bytes) = engine.click_report(krad_session::MouseAction::Press, at) {
                    engine.clear_selection();
                    self.dirty = true;
                    return self.write(bytes, WriteBy::App);
                }
            }
            TerminalAction::Press { at, report: false } => engine.start_selection(at),
            TerminalAction::Mouse { action, at } => {
                if let Some(bytes) = engine.click_report(action, at) {
                    return self.write(bytes, WriteBy::App);
                }
            }
            TerminalAction::ExtendSelection(at) => engine.extend_selection(at),
            TerminalAction::ClearSelection => engine.clear_selection(),
            TerminalAction::Search {
                query,
                direction,
                restart,
                found,
            } => {
                if restart {
                    engine.clear_selection();
                }
                let hit = !query.is_empty() && engine.search(&query, direction);
                answer(found, hit);
            }
            TerminalAction::CopySelection(text) => answer(text, engine.selection_text()),
            TerminalAction::RecentLines { lines, tail } => {
                answer(tail, Some(engine.recent_lines(lines)));
            }
            TerminalAction::Hooked => {
                self.hooked = true;
                self.publish_status();
            }
        }
        self.dirty = true;
        Written::Sent
    }

    pub(crate) fn recolor(&mut self) {
        self.dirty = true;
    }

    pub(crate) fn render(&mut self, palette: &AnsiPalette) -> bool {
        if !self.dirty {
            return false;
        }
        if !self.shared.is_wanted() {
            self.shared.mark_stale();
            return false;
        }
        let engine = &self.session.engine;
        let screen = engine.screen(palette);
        self.images.retain(|id, shown| {
            screen
                .images
                .iter()
                .any(|placed| placed.id == *id && placed.generation == shown.generation)
        });
        for placed in &screen.images {
            if self.images.contains_key(&placed.id) {
                continue;
            }
            if let Some(pixels) = engine.image_pixels(placed.id) {
                let shown = ShownImage {
                    generation: placed.generation,
                    pixels: Arc::new(pixels),
                };
                self.images.insert(placed.id, shown);
            }
        }
        self.shared.publish(TerminalFrame {
            screen,
            images: self.images.clone(),
            cell: Some(engine.cell_pixels()),
        });
        self.dirty = false;
        true
    }

    pub(crate) fn meta_due(&self, now: Instant, settling: bool) -> bool {
        match self.scrollback {
            Scrollback::Saved => false,
            Scrollback::Due => true,
            Scrollback::Stale => {
                settling
                    || now.saturating_duration_since(self.scrollback_read) >= SCROLLBACK_REFRESH
            }
        }
    }

    pub(crate) fn meta(&mut self, now: Instant) -> TerminalMeta {
        self.scrollback = Scrollback::Saved;
        self.scrollback_read = now;
        TerminalMeta {
            cwd: self.session.cwd.clone().unwrap_or_default(),
            scrollback: self.session.engine.scrollback(SCROLLBACK_PERSIST_LINES),
        }
    }

    pub(crate) fn kill(self) {
        self.session.cancelled.store(true, Ordering::SeqCst);
        if let Some(killer) = self.session.killer {
            killer.kill();
        }
    }

    fn publish_status(&mut self) {
        let status = TerminalStatus {
            run: self.session.state.clone(),
            hooked: self.hooked,
            bracketed_paste: self.session.engine.bracketed_paste(),
            mouse_tracking: self.session.engine.mouse_tracking(),
        };
        if status != self.published {
            self.shared.set_status(status.clone());
            self.published = status;
        }
    }
}

fn answer<T>(reply: futures::channel::oneshot::Sender<T>, value: T) {
    if reply.send(value).is_err() {
        tracing::trace!("a client left before the terminal answered");
    }
}
