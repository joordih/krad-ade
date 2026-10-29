mod look_sheet;
mod sheets;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use krad_domain::{EnvironmentName, GroupId, SessionId};

use super::native_alert::Confirmation;
use super::EnvironmentShell;

const DELETE_ENVIRONMENT_TITLE: &str = "Delete environment?";
const DELETE_ENVIRONMENT_TEXT: &str = "Its terminals and sessions close with it.";
const DELETE_GROUP_TITLE: &str = "Delete group?";
const DELETE_GROUP_TEXT: &str =
    "Its terminals and sessions close with it. The folder stays on disk.";
const DELETE_CONFIRM: &str = "Delete";
const DELETE_CANCEL: &str = "Cancel";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NameTarget {
    Environment(EnvironmentName),
    Group(GroupId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DeleteTarget {
    Environment(EnvironmentName),
    Group(GroupId),
}

impl DeleteTarget {
    const fn title(&self) -> &'static str {
        match self {
            Self::Environment(_) => DELETE_ENVIRONMENT_TITLE,
            Self::Group(_) => DELETE_GROUP_TITLE,
        }
    }

    const fn text(&self) -> &'static str {
        match self {
            Self::Environment(_) => DELETE_ENVIRONMENT_TEXT,
            Self::Group(_) => DELETE_GROUP_TEXT,
        }
    }
}

pub(super) struct NameDialog {
    target: NameTarget,
    field: Entity<InputState>,
    _events: Subscription,
}

#[derive(Default)]
pub(super) enum NameDialogs {
    #[default]
    Closed,
    Name(NameDialog),
    Delete(DeleteTarget),
    Look(SessionId),
}

impl EnvironmentShell {
    pub(super) fn ask_name(
        &mut self,
        target: NameTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(current) = self.current_name(&target) else {
            return;
        };
        let field = cx.new(|cx| InputState::new(window, cx).default_value(current));
        let focus_target = field.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus_target.update(cx, |field, cx| field.focus(window, cx));
        });
        let events = cx.subscribe_in(
            &field,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.commit_name(cx);
                }
            },
        );
        self.name_dialogs = NameDialogs::Name(NameDialog {
            target,
            field,
            _events: events,
        });
        cx.notify();
    }

    fn current_name(&self, target: &NameTarget) -> Option<String> {
        match target {
            NameTarget::Environment(name) => Some(self.env_label(name)),
            NameTarget::Group(id) => self
                .envs
                .get(&self.active)
                .and_then(|live| live.data.group(*id))
                .map(|group| group.name.clone()),
        }
    }

    fn commit_name(&mut self, cx: &mut Context<Self>) {
        let NameDialogs::Name(dialog) = std::mem::take(&mut self.name_dialogs) else {
            return;
        };
        let typed = dialog.field.read(cx).value().trim().to_owned();
        if !typed.is_empty() {
            match dialog.target {
                NameTarget::Environment(name) => {
                    if let Some(live) = self.envs.get_mut(&name) {
                        live.data.title = Some(typed);
                    }
                }
                NameTarget::Group(id) => {
                    let env = self.active.clone();
                    if let Some(group) = self
                        .envs
                        .get_mut(&env)
                        .and_then(|live| live.data.groups.iter_mut().find(|group| group.id == id))
                    {
                        group.name = typed;
                    }
                }
            }
            self.store();
        }
        let entries = self.switcher_menu_entries();
        self.replace_menu_entries(entries);
        cx.notify();
    }

    fn close_name_dialog(&mut self, cx: &mut Context<Self>) {
        self.name_dialogs = NameDialogs::Closed;
        cx.notify();
    }

    pub(super) fn ask_delete(
        &mut self,
        target: DeleteTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let detail = format!("{}\n\n{}", self.delete_subject(&target), target.text());
        let confirmation = Confirmation {
            title: target.title(),
            detail: &detail,
            confirm: DELETE_CONFIRM,
            cancel: DELETE_CANCEL,
        };
        let asked_for = target.clone();
        let asked = Self::confirm_natively(&confirmation, window, cx, |this, confirmed, _, cx| {
            if confirmed {
                this.delete(asked_for, cx);
            }
        });
        if !asked {
            self.name_dialogs = NameDialogs::Delete(target);
            cx.notify();
        }
    }

    fn delete_subject(&self, target: &DeleteTarget) -> String {
        match target {
            DeleteTarget::Environment(name) => self.env_label(name),
            DeleteTarget::Group(id) => self
                .current_name(&NameTarget::Group(*id))
                .unwrap_or_default(),
        }
    }

    fn delete(&mut self, target: DeleteTarget, cx: &mut Context<Self>) {
        self.name_dialogs = NameDialogs::Closed;
        match target {
            DeleteTarget::Environment(name) => self.close_env(&name, cx),
            DeleteTarget::Group(id) => {
                self.select_group(id, cx);
                self.remove_selected_group(cx);
            }
        }
        cx.notify();
    }

    pub(super) fn customize_session(&mut self, session: SessionId, cx: &mut Context<Self>) {
        self.name_dialogs = NameDialogs::Look(session);
        cx.notify();
    }
}
