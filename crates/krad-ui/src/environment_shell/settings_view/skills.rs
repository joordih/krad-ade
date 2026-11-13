mod filters;
mod row;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AnyElement, AppContext as _, Context, Entity, Subscription, Window};
use krad_domain::{
    enabled_count, LibraryFilter, LibrarySkill, SkillHarness, SkillKey, SkillKind, ToastKind,
};
use krad_protocol::{Answer, RepoAsk};
use std::path::PathBuf;

use super::super::repo_asks::done;
use super::sections::{empty_row, pane_column, settings_section};
use super::{panel_card, EnvironmentShell, SettingsPane};
use crate::theme::palette;

const SEARCH_PLACEHOLDER: &str = "Search skills, agents and commands";
const LIBRARY_TITLE: &str = "Library";
const DETECTING_TEXT: &str = "Looking for skills…";
const EMPTY_TEXT: &str = "No skills, agents or commands found.";
const NO_MATCH_TEXT: &str = "Nothing matches these filters.";
const ENABLED_MIDDLE: &str = " of ";
const ENABLED_TAIL: &str = " on. Turned off ones stay on disk but are not offered from the composer or given to Gateway chats.";
const REVEAL_FAILED: &str = "Couldn't reveal the file";

#[derive(Default)]
enum Library {
    #[default]
    Detecting,
    Ready(Vec<LibrarySkill>),
}

pub(in crate::environment_shell) struct SkillsUi {
    search: Entity<InputState>,
    kind: Option<SkillKind>,
    harness: Option<SkillHarness>,
    library: Library,
    root: Option<PathBuf>,
    _search_edit: Subscription,
}

impl SkillsUi {
    fn filter(&self, query: String) -> LibraryFilter {
        LibraryFilter {
            query,
            kind: self.kind,
            harness: self.harness,
        }
    }
}

fn enabled_hint(enabled: usize, total: usize) -> String {
    format!("{enabled}{ENABLED_MIDDLE}{total}{ENABLED_TAIL}")
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn skills_ui(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> SkillsUi {
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(SEARCH_PLACEHOLDER.to_owned()));
        let edit = cx.subscribe(&search, |_: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        SkillsUi {
            search,
            kind: None,
            harness: None,
            library: Library::Detecting,
            root: None,
            _search_edit: edit,
        }
    }

    fn skills_ui_mut(&mut self) -> Option<&mut SkillsUi> {
        self.settings_ui.as_mut().map(|ui| &mut ui.skills)
    }

    pub(in crate::environment_shell) fn refresh_skill_library(&mut self, cx: &mut Context<Self>) {
        let shown = self
            .settings_ui
            .as_ref()
            .is_some_and(|ui| ui.pane == SettingsPane::Skills);
        if !shown {
            return;
        }
        let root = self.active_group().map(|group| group.root.clone());
        if let Some(ui) = self.skills_ui_mut() {
            ui.root.clone_from(&root);
            if !matches!(ui.library, Library::Ready(_)) {
                ui.library = Library::Detecting;
            }
        }
        let wanted = root.clone();
        let detecting = self.ask_active_repo(
            RepoAsk::Skills { root },
            |answer| match answer {
                Answer::Skills { skills } => Some(skills),
                _ => None,
            },
            cx,
        );
        cx.spawn(async move |this, cx| {
            let found = detecting.await;
            this.update(cx, |shell, cx| {
                match found {
                    Ok(skills) => {
                        if let Some(ui) = shell.skills_ui_mut().filter(|ui| ui.root == wanted) {
                            ui.library = Library::Ready(skills);
                        }
                    }
                    Err(error) => shell.toast_failure(&error, cx),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn set_skill_enabled(&mut self, key: SkillKey, enabled: bool, cx: &mut Context<Self>) {
        self.settings.skills.set_enabled(key, enabled);
        self.save_settings(cx);
        cx.notify();
    }

    fn set_skill_kind_filter(&mut self, kind: Option<SkillKind>, cx: &mut Context<Self>) {
        if let Some(ui) = self.skills_ui_mut() {
            ui.kind = kind;
        }
        cx.notify();
    }

    fn set_skill_harness_filter(&mut self, harness: Option<SkillHarness>, cx: &mut Context<Self>) {
        if let Some(ui) = self.skills_ui_mut() {
            ui.harness = harness;
        }
        cx.notify();
    }

    fn reveal_skill_file(&self, file: PathBuf, cx: &mut Context<Self>) {
        let revealing = self.ask_active_repo(RepoAsk::Reveal { path: file }, done, cx);
        cx.spawn(async move |this, cx| {
            let Err(error) = revealing.await else {
                return;
            };
            this.update(cx, |shell, cx| {
                shell.push_toast(ToastKind::Error, format!("{REVEAL_FAILED}: {error}"));
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn skills_pane(&self, ui: &SkillsUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let query = ui.search.read(cx).value().to_string();
        let filters = Self::skill_filters(ui, cx);
        let (rows, hint) = match &ui.library {
            Library::Detecting => (vec![empty_row(DETECTING_TEXT, pal)], None),
            Library::Ready(skills) if skills.is_empty() => (vec![empty_row(EMPTY_TEXT, pal)], None),
            Library::Ready(skills) => {
                let filter = ui.filter(query);
                let shown: Vec<AnyElement> = skills
                    .iter()
                    .enumerate()
                    .filter(|(_, skill)| skill.matches(&filter))
                    .map(|(index, skill)| self.skill_row(index, skill, cx))
                    .collect();
                let hint = enabled_hint(enabled_count(skills, &self.settings.skills), skills.len());
                if shown.is_empty() {
                    (vec![empty_row(NO_MATCH_TEXT, pal)], Some(hint))
                } else {
                    (shown, Some(hint))
                }
            }
        };
        pane_column([settings_section(
            LIBRARY_TITLE,
            hint.as_deref(),
            [filters, panel_card(rows, pal)],
            pal,
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_counts_the_enabled_entries() {
        assert!(enabled_hint(3, 5).starts_with("3 of 5 on."));
    }
}
