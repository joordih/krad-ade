use gpui_kit::{Context, PromptButton, PromptLevel, Window};
use krad_extensions::api::limits::MAX_PENDING_PROMPTS;
use krad_extensions::api::protocol::{
    Answer, Confirm, ConfirmParams, InputBox, InputBoxParams, Pick, PickItem, PickMode, PickParams,
};
use krad_extensions::api::view::Tone;
use krad_extensions::program::CallId;
use std::collections::{BTreeSet, VecDeque};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement, MenuSearch};
use super::super::name_dialogs::{NameDialogs, NameTarget};
use super::super::EnvironmentShell;
use super::commands::{icon_of, ExtensionMenu};
use super::runtime_install::PendingInstall;
use super::SlotKey;

const NATIVE_ALERTS: bool = cfg!(target_os = "macos");
const CONFIRMED: usize = 0;
const CANCEL_LABEL: &str = "Cancel";
const DONE_LABEL: &str = "Done";
const SEARCHABLE_FROM: usize = 8;
const MENU_ORIGIN_X_SHARE: f32 = 0.5;
const MENU_ORIGIN_Y_SHARE: f32 = 0.3;

pub(super) enum Prompt {
    Confirm(SlotKey, CallId, ConfirmParams),
    Pick(SlotKey, CallId, PickParams),
    Many(ManyPick),
    Input(SlotKey, CallId, InputBoxParams),
    InstallRuntime(PendingInstall),
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::environment_shell) enum PromptAnswer {
    Confirm(SlotKey, CallId, Answer),
    Pick(SlotKey, CallId, Option<Vec<String>>),
    Toggle(String),
    ManyDone,
    ManyCancelled,
    InstallRuntime(bool),
}

pub(super) struct ManyPick {
    key: SlotKey,
    id: CallId,
    params: PickParams,
    chosen: BTreeSet<String>,
}

#[derive(Default)]
pub(super) struct Prompts {
    queue: VecDeque<Prompt>,
    input: Option<(SlotKey, CallId)>,
    many: Option<ManyPick>,
    pub(super) install: Option<PendingInstall>,
}

const TOO_MANY_PROMPTS: &str = "the extension is already waiting for too many answers";

impl Prompt {
    fn asker(&self) -> Option<(&SlotKey, &CallId)> {
        match self {
            Self::Confirm(key, id, _) | Self::Pick(key, id, _) | Self::Input(key, id, _) => {
                Some((key, id))
            }
            Self::Many(many) => Some((&many.key, &many.id)),
            Self::InstallRuntime(_) => None,
        }
    }
}

fn waiting_for(queue: &VecDeque<Prompt>, key: &SlotKey) -> usize {
    queue
        .iter()
        .filter(|prompt| prompt.asker().is_some_and(|(asker, _)| asker == key))
        .count()
}

fn toggle(chosen: &mut BTreeSet<String>, value: String) {
    if !chosen.remove(&value) {
        chosen.insert(value);
    }
}

fn chosen_values(items: &[PickItem], chosen: &BTreeSet<String>) -> Vec<String> {
    items
        .iter()
        .filter(|item| chosen.contains(&item.value))
        .map(|item| item.value.clone())
        .collect()
}

impl EnvironmentShell {
    pub(super) fn queue_extension_prompt(&mut self, prompt: Prompt, cx: &mut Context<Self>) {
        if let Some((key, id)) = prompt.asker() {
            if waiting_for(&self.extensions.prompts.queue, key) >= MAX_PENDING_PROMPTS {
                let (key, id) = (key.clone(), id.clone());
                self.refuse_call(&key, id, TOO_MANY_PROMPTS);
                return;
            }
        }
        self.extensions.prompts.queue.push_back(prompt);
        cx.notify();
    }

    pub(in crate::environment_shell) fn flush_extension_prompts(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restore_extension_views(cx);
        let busy = self.context_menu.is_some()
            || self.extensions.prompts.input.is_some()
            || self.extensions.prompts.install.is_some()
            || !matches!(self.name_dialogs, NameDialogs::Closed);
        if busy {
            return;
        }
        let Some(prompt) = self.extensions.prompts.queue.pop_front() else {
            return;
        };
        match prompt {
            Prompt::Confirm(key, id, params) => self.show_confirm(key, id, &params, window, cx),
            Prompt::Pick(key, id, params) if params.mode == PickMode::Many => {
                let many = ManyPick {
                    key,
                    id,
                    params,
                    chosen: BTreeSet::new(),
                };
                self.show_many(many, window, cx);
            }
            Prompt::Pick(key, id, params) => self.show_pick(&key, &id, &params, window, cx),
            Prompt::Many(many) => self.show_many(many, window, cx),
            Prompt::InstallRuntime(pending) => self.show_runtime_install(pending, window, cx),
            Prompt::Input(key, id, params) => {
                self.extensions.prompts.input = Some((key, id));
                self.ask_name(
                    NameTarget::ExtensionInput {
                        title: params.title,
                        value: params.value,
                    },
                    window,
                    cx,
                );
            }
        }
    }

    fn menu_origin(window: &Window) -> (f32, f32) {
        let size = window.viewport_size();
        (
            size.width.as_f32() * MENU_ORIGIN_X_SHARE,
            size.height.as_f32() * MENU_ORIGIN_Y_SHARE,
        )
    }

    fn show_confirm(
        &mut self,
        key: SlotKey,
        id: CallId,
        params: &ConfirmParams,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if NATIVE_ALERTS {
            let level = match params.tone {
                Tone::Error | Tone::Warning => PromptLevel::Warning,
                Tone::Neutral | Tone::Info | Tone::Success => PromptLevel::Info,
            };
            let answer = window.prompt(
                level,
                &params.title,
                params.detail.as_deref(),
                &[
                    PromptButton::ok(params.confirm.clone()),
                    PromptButton::cancel(CANCEL_LABEL),
                ],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                let confirmed = matches!(answer.await, Ok(CONFIRMED));
                this.update(cx, |shell, _| {
                    let answer = if confirmed {
                        Answer::Confirmed
                    } else {
                        Answer::Cancelled
                    };
                    shell.answer_call::<Confirm>(&key, id, &answer);
                })
                .unwrap_or_default();
            })
            .detach();
            return;
        }
        let answer = |answer: Answer| {
            MenuAction::Extension(Box::new(ExtensionMenu::Answer(PromptAnswer::Confirm(
                key.clone(),
                id.clone(),
                answer,
            ))))
        };
        let mut entries = vec![MenuEntry::Header(params.title.clone())];
        entries.extend(params.detail.clone().map(MenuEntry::Note));
        let confirm = if matches!(params.tone, Tone::Error | Tone::Warning) {
            MenuEntry::destructive(
                params.confirm.clone(),
                crate::components::IconName::Close,
                answer(Answer::Confirmed),
            )
        } else {
            MenuEntry::item(params.confirm.clone(), None, answer(Answer::Confirmed))
        };
        entries.push(confirm);
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::item(
            CANCEL_LABEL,
            None,
            answer(Answer::Cancelled),
        ));
        self.open_menu(Self::menu_origin(window), MenuPlacement::Below, entries, cx);
    }

    fn show_pick(
        &mut self,
        key: &SlotKey,
        id: &CallId,
        params: &PickParams,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = |picked: Option<Vec<String>>| {
            MenuAction::Extension(Box::new(ExtensionMenu::Answer(PromptAnswer::Pick(
                key.clone(),
                id.clone(),
                picked,
            ))))
        };
        let mut entries = vec![MenuEntry::Header(params.title.clone())];
        entries.extend(params.items.iter().map(|item| {
            let entry = MenuEntry::item(
                item.label.clone(),
                item.icon.map(|icon| icon_of(Some(icon))),
                answer(Some(vec![item.value.clone()])),
            );
            match &item.detail {
                Some(detail) => entry.with_detail(detail.clone()),
                None => entry,
            }
        }));
        let cancel = MenuEntry::item(CANCEL_LABEL, None, answer(None));
        let origin = Self::menu_origin(window);
        if params.items.len() >= SEARCHABLE_FROM {
            let search = MenuSearch {
                placeholder: "",
                create: None,
                footer: vec![cancel],
            };
            self.open_searchable_menu(origin, entries, search, window, cx);
            return;
        }
        entries.push(MenuEntry::Separator);
        entries.push(cancel);
        self.open_menu(origin, MenuPlacement::Below, entries, cx);
    }

    fn show_many(&mut self, many: ManyPick, window: &mut Window, cx: &mut Context<Self>) {
        let answer =
            |answer: PromptAnswer| MenuAction::Extension(Box::new(ExtensionMenu::Answer(answer)));
        let mut entries = vec![MenuEntry::Header(many.params.title.clone())];
        entries.extend(many.params.items.iter().map(|item| {
            let entry = MenuEntry::checked(
                item.label.clone(),
                many.chosen.contains(&item.value),
                answer(PromptAnswer::Toggle(item.value.clone())),
            );
            match &item.detail {
                Some(detail) => entry.with_detail(detail.clone()),
                None => entry,
            }
        }));
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::item(
            DONE_LABEL,
            None,
            answer(PromptAnswer::ManyDone),
        ));
        entries.push(MenuEntry::item(
            CANCEL_LABEL,
            None,
            answer(PromptAnswer::ManyCancelled),
        ));
        self.extensions.prompts.many = Some(many);
        self.open_menu(Self::menu_origin(window), MenuPlacement::Below, entries, cx);
    }

    fn answer_many(&mut self, answer: &PromptAnswer, cx: &mut Context<Self>) {
        let Some(mut many) = self.extensions.prompts.many.take() else {
            return;
        };
        match answer {
            PromptAnswer::Toggle(value) => {
                toggle(&mut many.chosen, value.clone());
                self.extensions.prompts.queue.push_front(Prompt::Many(many));
            }
            PromptAnswer::ManyDone => {
                let picked = chosen_values(&many.params.items, &many.chosen);
                self.answer_call::<Pick>(&many.key, many.id, &Some(picked));
            }
            _ => self.answer_call::<Pick>(&many.key, many.id, &None),
        }
        cx.notify();
    }

    pub(super) fn answer_prompt(
        &mut self,
        answer: &PromptAnswer,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match answer {
            PromptAnswer::Confirm(key, id, answer) => {
                self.answer_call::<Confirm>(key, id.clone(), answer);
            }
            PromptAnswer::Pick(key, id, picked) => {
                self.answer_call::<Pick>(key, id.clone(), picked);
            }
            PromptAnswer::Toggle(_) | PromptAnswer::ManyDone | PromptAnswer::ManyCancelled => {
                self.answer_many(answer, cx);
            }
            PromptAnswer::InstallRuntime(install) => self.runtime_install_answered(*install, cx),
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn extension_input_answered(
        &mut self,
        value: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some((key, id)) = self.extensions.prompts.input.take() else {
            return;
        };
        self.answer_call::<InputBox>(&key, id, &value.map(str::to_owned));
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
