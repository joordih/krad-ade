mod inbox;
mod jump;
mod mirror;
mod notifications;

use gpui_kit::{AnyWindowHandle, App};
use krad_domain::{
    AgentState, AttentionKind, AttentionTarget, Circumstances, EnvironmentName, EnvironmentView,
    InboxFilter, SessionAttention, Sight, TerminalId, Visibility, WindowFocus,
};
use krad_protocol::{AttentionCommand, AttentionNotice, Command};
use std::collections::HashMap;

use super::EnvironmentShell;
use crate::app_badge::{self, BadgeSignal};
use crate::{attention_sound, local_clock};
pub(super) use mirror::AttentionMirror;
use notifications::{Announcement, Answers, Delivered};

const NO_BADGE: usize = 0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Flash {
    #[default]
    Idle,
    Wanted,
}

pub(super) struct AttentionUi {
    window: WindowFocus,
    pub(super) window_handle: Option<AnyWindowHandle>,
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
        let target = AttentionTarget::new(env.clone(), id);
        self.tell_attention(AttentionCommand::Observe {
            title: self.agent_title(&target).unwrap_or_default(),
            target,
            previous,
            next,
            waiting_for,
            sight: self.sight_of(env, id),
        });
    }

    pub(super) fn raise_attention(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        kind: AttentionKind,
    ) {
        let target = AttentionTarget::new(env.clone(), id);
        self.tell_attention(AttentionCommand::Raise {
            title: self.agent_title(&target).unwrap_or_default(),
            target,
            kind,
            sight: self.sight_of(env, id),
        });
    }

    pub(super) fn raise_front_ask(&mut self, env: &EnvironmentName, id: TerminalId) {
        if let Some(front) = self.front_ask(env, id) {
            self.raise_attention(env, id, front.kind);
        }
    }

    fn tell_attention(&self, command: AttentionCommand) {
        self.runtime.handle.post(Command::Attention(command));
    }

    pub(super) fn attention_raised(&mut self, notice: AttentionNotice) {
        self.runtime.view.attention.raised(&notice);
        let AttentionNotice {
            target,
            kind,
            title,
            ..
        } = notice;
        let session = self.session_attention_of(&target);
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
        if self.runtime.view.attention.queue.contains(&target) {
            self.tell_attention(AttentionCommand::Seen { target });
        }
    }

    pub(super) fn forget_attention(&mut self, env: &EnvironmentName, id: TerminalId) {
        let target = AttentionTarget::new(env.clone(), id);
        if self.runtime.view.attention.queue.contains(&target) {
            self.runtime.view.attention.left(&target);
            self.tell_attention(AttentionCommand::Dismiss { target });
        }
    }

    fn is_open(&self, target: &AttentionTarget) -> bool {
        self.envs.get(&target.environment).is_some_and(|live| {
            live.data
                .nodes
                .iter()
                .any(|node| node.id == target.terminal)
        })
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
        let closed: Vec<AttentionTarget> = self
            .runtime
            .view
            .attention
            .queue
            .ordered()
            .into_iter()
            .map(|entry| &entry.target)
            .filter(|target| !self.is_open(target))
            .cloned()
            .collect();
        for target in closed {
            self.forget_attention(&target.environment, target.terminal);
        }
        self.deliver_notifications(cx);
        self.show_badge();
        self.flash_window(cx);
    }

    fn show_badge(&mut self) {
        let settings = self.settings.attention;
        let count = if settings.badge.is_on() {
            self.runtime
                .view
                .attention
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
