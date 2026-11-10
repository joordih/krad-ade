use gpui_kit::{
    App, Context, SharedString, SystemNotification, SystemNotificationAction,
    SystemNotificationResponse, Window,
};
use krad_domain::{AttentionKind, AttentionTarget};

use super::super::ask::PermissionReply;
use super::super::EnvironmentShell;

const TAG_PREFIX: &str = "krad-attention";
const TAG_SEPARATOR: &str = ":";
const PERMISSION_TITLE: &str = "An agent asks for permission";
const ASK_TITLE: &str = "An agent needs you";
const ERROR_TITLE: &str = "An agent ran into an error";
const FINISHED_TITLE: &str = "An agent finished";
const BODY_SEPARATOR: &str = " · ";
const LINE_BREAK: &str = "\n";
const APPROVE_ID: &str = "approve";
const APPROVE_LABEL: &str = "Approve";
const DENY_ID: &str = "deny";
const DENY_LABEL: &str = "Deny";
const ASKS_SOMETHING_ELSE: &str = "The agent asks something else now; open it to answer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Answers {
    Offered,
    Withheld,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Announcement {
    pub(super) target: AttentionTarget,
    pub(super) kind: AttentionKind,
    pub(super) title: String,
    pub(super) subject: Option<String>,
    pub(super) answers: Answers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Delivered {
    kind: AttentionKind,
    subject: Option<String>,
}

const fn headline(kind: AttentionKind) -> &'static str {
    match kind {
        AttentionKind::Permission => PERMISSION_TITLE,
        AttentionKind::Ask => ASK_TITLE,
        AttentionKind::Error => ERROR_TITLE,
        AttentionKind::Finished => FINISHED_TITLE,
    }
}

fn answer_actions() -> Vec<SystemNotificationAction> {
    [(APPROVE_ID, APPROVE_LABEL), (DENY_ID, DENY_LABEL)]
        .into_iter()
        .map(|(id, label)| SystemNotificationAction {
            id: SharedString::from(id),
            label: SharedString::from(label),
        })
        .collect()
}

fn reply_of(action: &str) -> Option<PermissionReply> {
    match action {
        APPROVE_ID => Some(PermissionReply::Approve),
        DENY_ID => Some(PermissionReply::Deny),
        _ => None,
    }
}

pub(super) fn notification_tag(target: &AttentionTarget) -> SharedString {
    SharedString::from(
        [
            TAG_PREFIX,
            target.environment.as_str(),
            &target.terminal.as_u64().to_string(),
        ]
        .join(TAG_SEPARATOR),
    )
}

impl Announcement {
    fn notification(&self) -> SystemNotification {
        let place = [self.title.as_str(), self.target.environment.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(BODY_SEPARATOR);
        let body = match &self.subject {
            Some(subject) if !subject.is_empty() => [place, subject.clone()].join(LINE_BREAK),
            Some(_) | None => place,
        };
        let actions = match (self.answers, self.kind) {
            (Answers::Offered, AttentionKind::Permission) => answer_actions(),
            _ => Vec::new(),
        };
        SystemNotification {
            tag: notification_tag(&self.target),
            title: SharedString::from(headline(self.kind)),
            body: SharedString::from(body),
            actions,
        }
    }
}

impl EnvironmentShell {
    pub(super) fn deliver_notifications(&mut self, cx: &mut App) {
        for announcement in std::mem::take(&mut self.attention.outbox) {
            if !self.attention.queue.contains(&announcement.target) {
                continue;
            }
            cx.show_system_notification(announcement.notification());
            self.attention.delivered.insert(
                announcement.target,
                Delivered {
                    kind: announcement.kind,
                    subject: announcement.subject,
                },
            );
        }
        let queue = &self.attention.queue;
        let answered: Vec<AttentionTarget> = self
            .attention
            .delivered
            .keys()
            .filter(|target| !queue.contains(target))
            .cloned()
            .collect();
        for target in answered {
            cx.dismiss_system_notification(&notification_tag(&target));
            self.attention.delivered.remove(&target);
        }
    }

    pub(in crate::environment_shell) fn answer_attention_notifications(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let handle = window.window_handle();
        self.attention.window_handle = Some(handle);
        let shell = cx.weak_entity();
        cx.on_system_notification_response(move |response: SystemNotificationResponse, cx| {
            let shell = shell.clone();
            let answered = handle.update(cx, |_, window, cx| {
                shell.update(cx, |shell, cx| {
                    shell.answer_notification(&response, window, cx);
                })
            });
            if answered.is_err() {
                tracing::debug!("notification answered after its window closed");
            }
        });
    }

    fn answer_notification(
        &mut self,
        response: &SystemNotificationResponse,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self
            .attention
            .delivered
            .keys()
            .find(|target| notification_tag(target) == response.tag)
            .cloned();
        let reply = response.action_id.as_deref().and_then(reply_of);
        match (target, reply) {
            (Some(target), Some(reply)) => self.reply_from_notification(&target, reply, cx),
            (Some(target), None) => {
                window.activate_window();
                self.open_attention(&target, window, cx);
            }
            (None, _) => {
                window.activate_window();
                self.open_next_waiting(window, cx);
            }
        }
    }

    fn reply_from_notification(
        &mut self,
        target: &AttentionTarget,
        reply: PermissionReply,
        cx: &mut Context<Self>,
    ) {
        let notified = self.attention.delivered.get(target).cloned();
        let front = self.front_ask(&target.environment, target.terminal);
        let unchanged = matches!(
            (&notified, &front),
            (Some(notified), Some(front))
                if notified.kind == AttentionKind::Permission
                    && front.kind == AttentionKind::Permission
                    && notified.subject.as_deref() == Some(front.subject.as_str())
        );
        if !unchanged {
            self.push_toast(ASKS_SOMETHING_ELSE.to_owned());
            cx.notify();
            return;
        }
        if self.active != target.environment {
            self.switch_env(&target.environment, cx);
        }
        if !self.reply_to_permission(target.terminal, reply, cx) {
            self.push_toast(ASKS_SOMETHING_ELSE.to_owned());
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{EnvironmentName, TerminalId};

    fn target(env: &str, id: u64) -> AttentionTarget {
        AttentionTarget::new(EnvironmentName::new(env.to_owned()), TerminalId(id))
    }

    fn permission() -> Announcement {
        Announcement {
            target: target("payments", 4),
            kind: AttentionKind::Permission,
            title: "claude".to_owned(),
            subject: Some("Bash cargo test".to_owned()),
            answers: Answers::Offered,
        }
    }

    #[test]
    fn a_permission_shows_what_it_asks_with_approve_and_deny() {
        let shown = permission().notification();
        assert_eq!(shown.title.as_ref(), PERMISSION_TITLE);
        assert_eq!(shown.body.as_ref(), "claude · payments\nBash cargo test");
        let ids: Vec<&str> = shown
            .actions
            .iter()
            .map(|action| action.id.as_ref())
            .collect();
        assert_eq!(ids, [APPROVE_ID, DENY_ID]);
        for action in &shown.actions {
            assert!(reply_of(&action.id).is_some());
        }
    }

    #[test]
    fn answers_are_offered_only_for_permissions_when_allowed() {
        let withheld = Announcement {
            answers: Answers::Withheld,
            ..permission()
        };
        assert_eq!(withheld.notification().actions, Vec::new());
        for kind in [
            AttentionKind::Ask,
            AttentionKind::Error,
            AttentionKind::Finished,
        ] {
            let other = Announcement {
                kind,
                subject: None,
                ..permission()
            };
            let shown = other.notification();
            assert_eq!(shown.actions, Vec::new(), "{kind:?}");
            assert_eq!(shown.title.as_ref(), headline(kind));
            assert_eq!(shown.body.as_ref(), "claude · payments");
        }
    }

    #[test]
    fn unknown_actions_answer_nothing() {
        assert_eq!(reply_of("dismiss"), None);
        assert_eq!(reply_of(APPROVE_ID), Some(PermissionReply::Approve));
        assert_eq!(reply_of(DENY_ID), Some(PermissionReply::Deny));
    }

    #[test]
    fn each_agent_replaces_only_its_own_notification() {
        assert_eq!(
            notification_tag(&target("main", 1)),
            notification_tag(&target("main", 1))
        );
        assert_ne!(
            notification_tag(&target("main", 1)),
            notification_tag(&target("main", 2))
        );
        assert_ne!(
            notification_tag(&target("main", 1)),
            notification_tag(&target("other", 1))
        );
    }
}
