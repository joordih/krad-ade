mod brief;
mod cue;
mod flow;
mod logbook;
mod parts;
mod picker;
mod schedule;
mod steps;
mod workspace;

use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Window};
use krad_domain::{
    AutomationDraft, AutomationId, AutomationKey, Switch, ToastKind, Trigger, WorkingCopy,
};
use std::time::Duration;

use super::super::EnvironmentShell;
use super::model::{owner_for, CueShelf};
use super::AutomationMenu;
use schedule::ScheduleRow;
use steps::{flawed_step, Step};

const INSTRUCTION_ROWS: usize = 8;
const NAME_PLACEHOLDER: &str = "Name this automation";
const BRIEF_PLACEHOLDER: &str =
    "Describe the job in plain words, the way you would brief a teammate...";
const LAUNCHED: &str = "Launched and on duty";
const SAVED: &str = "Changes saved";
const OWNER_GONE: &str = "That Kraddie is no longer around";
const CELEBRATION: Duration = Duration::from_millis(1400);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Flaw {
    step: Step,
    message: String,
    attempt: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cheer {
    Launched,
    Saved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CuePicker {
    shelf: Option<CueShelf>,
    opened: u64,
}

pub(in crate::environment_shell) struct AutomationEditor {
    editing: Option<AutomationKey>,
    draft: AutomationDraft,
    schedules: Vec<ScheduleRow>,
    name: Entity<InputState>,
    brief: Entity<TextareaState>,
    fine_print_open: bool,
    picker: Option<CuePicker>,
    flaw: Option<Flaw>,
    attempts: u64,
    celebrating: Option<(u64, Cheer)>,
}

impl AutomationEditor {
    fn new(
        editing: Option<AutomationKey>,
        mut draft: AutomationDraft,
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) -> Self {
        let schedules = draft
            .triggers
            .iter()
            .filter_map(Trigger::schedule)
            .map(|schedule| ScheduleRow::new(*schedule, window, cx))
            .collect();
        draft
            .triggers
            .retain(|trigger| trigger.schedule().is_none());
        let (name, brief) = (draft.name.clone(), draft.instructions.clone());
        Self {
            editing,
            draft,
            schedules,
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(NAME_PLACEHOLDER)
                    .default_value(name)
            }),
            brief: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(INSTRUCTION_ROWS)
                    .placeholder(BRIEF_PLACEHOLDER)
                    .default_value(brief)
            }),
            fine_print_open: false,
            picker: None,
            flaw: None,
            attempts: 0,
            celebrating: None,
        }
    }

    pub(super) const fn editing(&self) -> Option<&AutomationKey> {
        self.editing.as_ref()
    }

    pub(super) const fn set_switch(&mut self, switch: Switch) {
        self.draft.switch = switch;
    }

    pub(super) fn apply(
        &mut self,
        choice: AutomationMenu,
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) {
        let settles = self
            .flaw
            .as_ref()
            .is_some_and(|flaw| flaw.step.settled_by(&choice));
        match choice {
            AutomationMenu::AddTrigger(trigger) => self.draft.toggle_trigger(trigger),
            AutomationMenu::AddSchedule(schedule) => {
                self.schedules.push(ScheduleRow::new(schedule, window, cx));
                self.picker = None;
            }
            AutomationMenu::Harness(harness) => self.draft.harness = harness,
            AutomationMenu::Effort(effort) => self.draft.effort = effort,
            AutomationMenu::Permission(mode) => self.draft.permission_mode = mode,
            AutomationMenu::Group(group) => {
                if group.is_some() {
                    self.draft.session.working_copy = WorkingCopy::ThisRepo;
                }
                self.draft.session.group = group;
            }
        }
        if settles {
            self.flaw = None;
        }
    }

    fn read(&self, cx: &Context<EnvironmentShell>) -> Result<AutomationDraft, &'static str> {
        let mut draft = self.draft.clone();
        draft.name = self.name.read(cx).value().to_string();
        draft.instructions = self.brief.read(cx).value().to_string();
        let schedules = self
            .schedules
            .iter()
            .map(|row| row.read(cx).map(|schedule| Trigger::Schedule { schedule }))
            .collect::<Result<Vec<_>, _>>()?;
        draft.triggers.splice(0..0, schedules);
        Ok(draft)
    }

    fn flag(&mut self, step: Step, message: String) {
        self.attempts += 1;
        self.flaw = Some(Flaw {
            step,
            message,
            attempt: self.attempts,
        });
    }

    fn toggle_picker(&mut self) {
        if self.picker.take().is_some() {
            return;
        }
        self.attempts += 1;
        self.picker = Some(CuePicker {
            shelf: None,
            opened: self.attempts,
        });
    }

    fn browse_cues(&mut self, target: Option<CueShelf>) {
        if let Some(picker) = &mut self.picker {
            picker.shelf = target;
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn start_automation(
        &mut self,
        template: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut draft = template
            .and_then(|id| self.automations.catalog.find(id))
            .map_or_else(AutomationDraft::default, AutomationDraft::from);
        let here = self.active_kraddie_workspace();
        draft.kraddie = owner_for(&self.kraddies.entries, here.as_ref());
        self.show_automation_editor(None, draft, window, cx);
    }

    pub(in crate::environment_shell) fn open_automation_editor(
        &mut self,
        key: &AutomationKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing_automation() == Some(key) {
            return;
        }
        let Some((_, entry)) = self.kraddies.automation(key) else {
            return;
        };
        let draft = AutomationDraft::from((key.kraddie.clone(), entry.automation.clone()));
        self.show_automation_editor(Some(key.clone()), draft, window, cx);
    }

    fn show_automation_editor(
        &mut self,
        editing: Option<AutomationKey>,
        draft: AutomationDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = AutomationEditor::new(editing, draft, window, cx);
        self.automations.editor = Some(Box::new(editor));
        self.refresh_code_host_logins(cx);
        cx.notify();
    }

    fn edit_automation_draft(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut AutomationEditor),
    ) {
        if let Some(editor) = &mut self.automations.editor {
            change(editor);
        }
        cx.notify();
    }

    fn save_automation_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.automations.editor else {
            return;
        };
        let draft = match editor.read(cx) {
            Ok(draft) => draft,
            Err(message) => {
                editor.flag(Step::Cue, message.to_owned());
                return cx.notify();
            }
        };
        let editing = editor.editing.clone();
        let validated = draft.validate(
            editing
                .as_ref()
                .map_or(AutomationId::FIRST, |key| key.automation),
        );
        let (kraddie, automation) = match validated {
            Ok(valid) => valid,
            Err(error) => {
                editor.flag(flawed_step(error), error.to_string());
                return cx.notify();
            }
        };
        let Some(key) = self.save_automation(&kraddie, automation, editing.as_ref(), cx) else {
            return self.toast(ToastKind::Warning, OWNER_GONE, cx);
        };
        self.toast(
            ToastKind::Success,
            if editing.is_some() { SAVED } else { LAUNCHED },
            cx,
        );
        let Some(editor) = &mut self.automations.editor else {
            return;
        };
        editor.editing = Some(key);
        editor.draft.kraddie = Some(kraddie);
        editor.flaw = None;
        editor.attempts += 1;
        let celebration = editor.attempts;
        let cheer = if editing.is_some() {
            Cheer::Saved
        } else {
            Cheer::Launched
        };
        editor.celebrating = Some((celebration, cheer));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CELEBRATION).await;
            this.update(cx, |shell, cx| {
                if let Some(editor) =
                    shell.automations.editor.as_mut().filter(|editor| {
                        editor.celebrating.is_some_and(|(id, _)| id == celebration)
                    })
                {
                    editor.celebrating = None;
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    fn retire_edited_automation(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.editing_automation().cloned() else {
            return;
        };
        self.delete_automation(&key, cx);
        self.close_automation_editor(cx);
    }
}
