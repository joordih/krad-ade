mod inbox;
mod jump;
mod notifications;

use gpui_kit::{AnyWindowHandle, App};
use krad_domain::{
    AgentState, AttentionKind, AttentionLog, AttentionQueue, AttentionRecord, AttentionTarget,
    Circumstances, EnvironmentName, EnvironmentView, InboxFilter, SessionAttention, Sight,
    StateChange, TerminalId, Visibility, WindowFocus,
};
use std::collections::HashMap;
use std::time::Instant;

use super::EnvironmentShell;
use crate::app_badge::{self, BadgeSignal};
use crate::{attention_sound, local_clock};
use notifications::{Announcement, Answers, Delivered};

const NO_BADGE: usize = 0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Flash {
    #[default]
    Idle,
    Wanted,
}

pub(super) struct AttentionUi {
    queue: AttentionQueue,
    log: AttentionLog,
    window: WindowFocus,
    window_handle: Option<AnyWindowHandle>,
    outbox: Vec<Announcement>,
    delivered: HashMap<AttentionTarget, Delivered>,
    badge: Option<usize>,
    flash: Flash,
    inbox: Visibility,
    filter: InboxFilter,
}

impl Default for AttentionUi {
    fn default() -> Self {
        Self {
            queue: AttentionQueue::default(),
            log: AttentionLog::default(),
            window: WindowFocus::default(),
            window_handle: None,
            outbox: Vec::new(),
            delivered: HashMap::new(),
            badge: None,
            flash: Flash::default(),
            inbox: Visibility::Hidden,
            filter: InboxFilter::default(),
        }
    }
}

impl EnvironmentShell {
    pub(super) fn observe_attention(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        previous: AgentState,
        next: AgentState,
    ) {
        let waiting_for = self
            .front_ask(env, id)
            .map_or(AttentionKind::Ask, |front| front.kind);
        let change = StateChange {
            target: AttentionTarget::new(env.clone(), id),
            previous,
            next,
            waiting_for,
            sight: self.sight_of(env, id),
            at: Instant::now(),
        };
        let target = change.target.clone();
        if let Some(kind) = self.attention.queue.observe(change) {
            self.announce(target, kind);
        }
    }

    pub(super) fn raise_attention(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        kind: AttentionKind,
    ) {
        let target = AttentionTarget::new(env.clone(), id);
        let sight = self.sight_of(env, id);
        if let Some(kind) = self
            .attention
            .queue
            .raise(target.clone(), kind, sight, Instant::now())
        {
            self.announce(target, kind);
        }
    }

    pub(super) fn raise_front_ask(&mut self, env: &EnvironmentName, id: TerminalId) {
        if let Some(front) = self.front_ask(env, id) {
            self.raise_attention(env, id, front.kind);
        }
    }

    fn announce(&mut self, target: AttentionTarget, kind: AttentionKind) {
        let live = self.envs.get(&target.environment);
        let title = live
            .and_then(|live| {
                live.data
                    .nodes
                    .iter()
                    .find(|node| node.id == target.terminal)
            })
            .map_or_else(String::new, |node| node.title.clone());
        let session = self.session_attention_of(&target);
        self.attention.log.record(AttentionRecord {
            target: target.clone(),
            kind,
            title: title.clone(),
            at: Instant::now(),
        });
        let circumstances = Circumstances {
            window: self.attention.window,
            session,
            now: local_clock::now(),
        };
        let settings = self.settings.attention;
        if let Some(sound) = settings.sound(kind, circumstances) {
            attention_sound::play(sound);
        }
        let away = self.attention.window == WindowFocus::Background;
        if away && app_badge::SIGNAL == BadgeSignal::Flash && settings.counts(kind, session) {
            self.attention.flash = Flash::Wanted;
        }
        if !settings.notifies(kind, circumstances) {
            return;
        }
        let subject = self
            .front_ask(&target.environment, target.terminal)
            .map(|front| front.subject);
        self.attention
            .outbox
            .retain(|queued| queued.target != target);
        self.attention.outbox.push(Announcement {
            target,
            kind,
            title,
            subject,
            answers: if settings.offers_answers(kind) {
                Answers::Offered
            } else {
                Answers::Withheld
            },
        });
    }

    fn session_attention_of(&self, target: &AttentionTarget) -> SessionAttention {
        self.envs
            .get(&target.environment)
            .map_or_else(SessionAttention::default, |live| {
                live.data.attention_of(target.terminal)
            })
    }

    pub(super) fn attention_seen(&mut self, env: &EnvironmentName, id: TerminalId) {
        let target = AttentionTarget::new(env.clone(), id);
        self.attention.queue.seen(&target);
    }

    pub(super) fn attention_records(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.attention
            .log
            .filtered(InboxFilter::All)
            .map(|record| (record.kind.display_name(), record.title.as_str()))
    }

    pub(super) fn forget_attention(&mut self, env: &EnvironmentName, id: TerminalId) {
        let target = AttentionTarget::new(env.clone(), id);
        self.attention.queue.forget(&target);
    }

    fn sight_of(&self, env: &EnvironmentName, id: TerminalId) -> Sight {
        let watched = self.attention.window == WindowFocus::Foreground
            && self.active == *env
            && self.envs.get(env).is_some_and(|live| {
                live.focused == Some(id) && live.data.view == EnvironmentView::Sessions
            });
        if watched {
            Sight::Watched
        } else {
            Sight::Unwatched
        }
    }

    pub(super) fn note_window_focus(&mut self, active: bool) {
        self.attention.window = if active {
            WindowFocus::Foreground
        } else {
            WindowFocus::Background
        };
        if !active {
            return;
        }
        let env = self.active.clone();
        let focused = self.envs.get(&env).and_then(|live| live.focused);
        if let Some(id) = focused {
            self.attention_seen(&env, id);
        }
    }

    pub(super) fn flush_attention(&mut self, cx: &mut App) {
        let envs = &self.envs;
        self.attention.queue.retain(|target| {
            envs.get(&target.environment).is_some_and(|live| {
                live.data
                    .nodes
                    .iter()
                    .any(|node| node.id == target.terminal)
            })
        });
        self.deliver_notifications(cx);
        self.show_badge();
        self.flash_window(cx);
    }

    fn show_badge(&mut self) {
        let settings = self.settings.attention;
        let count = if settings.badge.is_on() {
            self.attention
                .queue
                .ordered()
                .into_iter()
                .filter(|entry| {
                    settings.counts(entry.kind, self.session_attention_of(&entry.target))
                })
                .count()
        } else {
            NO_BADGE
        };
        if self.attention.badge != Some(count) {
            app_badge::show_count(count);
            self.attention.badge = Some(count);
        }
    }

    fn flash_window(&mut self, cx: &mut App) {
        if std::mem::take(&mut self.attention.flash) == Flash::Idle {
            return;
        }
        let Some(handle) = self.attention.window_handle else {
            return;
        };
        let flashed = handle.update(cx, |_, window, _| window.request_attention());
        if flashed.is_err() {
            tracing::debug!("attention flash after the window closed");
        }
    }
}
