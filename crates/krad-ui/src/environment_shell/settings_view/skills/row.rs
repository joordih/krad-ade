use gpui_kit::component::switch::Switch;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use krad_chat::{LibrarySkill, SkillPlace};
use std::path::PathBuf;

use super::super::super::dialog_parts::sheet_row_with;
use super::filters::{harness_mark, kind_mark};
use super::EnvironmentShell;
use crate::components::{chip, icon, tooltip_view, Button, ButtonVariant, IconName};
use crate::theme::{palette, ICON_SM, RADIUS_XS, SPACE_1, SPACE_2};

const ROW_PREFIX: &str = "skills-row";
const SWITCH_PREFIX: &str = "skills-switch";
const REVEAL_PREFIX: &str = "skills-reveal";
const SOURCE_PREFIX: &str = "skills-source";
const REVEAL_TOOLTIP: &str = "Reveal the file";
const CLASH_LABEL: &str = "Same name, other content";
const SCOPE_SEPARATOR: &str = " · ";
const PLUGIN_LEAD: &str = "plugin ";
const PLACE_SEPARATOR: &str = ": ";
const LINE_BREAK: &str = "\n";
const DISABLED_OPACITY: f32 = 0.6;

fn element_index(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

fn place_line(place: &SkillPlace) -> String {
    let plugin = place
        .plugin
        .as_ref()
        .map_or_else(String::new, |plugin| format!(" ({PLUGIN_LEAD}{plugin})"));
    format!(
        "{}{plugin}{PLACE_SEPARATOR}{}",
        place.harness.label(),
        place.file.display()
    )
}

pub(super) fn places_tooltip(skill: &LibrarySkill) -> String {
    skill
        .places
        .iter()
        .map(place_line)
        .collect::<Vec<_>>()
        .join(LINE_BREAK)
}

pub(super) fn scope_label(skill: &LibrarySkill) -> String {
    skill
        .scopes()
        .iter()
        .map(|scope| scope.label())
        .collect::<Vec<_>>()
        .join(SCOPE_SEPARATOR)
}

impl EnvironmentShell {
    pub(super) fn skill_row(
        &self,
        index: usize,
        skill: &LibrarySkill,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let id = element_index(index);
        let enabled = self.settings.skills.is_enabled(&skill.key);
        let key = skill.key.clone();
        let tooltip = SharedString::from(places_tooltip(skill));
        let sources = div()
            .id((SOURCE_PREFIX, id))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .children(skill.harnesses().into_iter().map(|harness| {
                let (mark, color) = harness_mark(harness, pal);
                icon(mark, ICON_SM, color)
            }))
            .tooltip(move |_, cx| tooltip_view(tooltip.clone(), cx));
        let revealed: Option<PathBuf> = self
            .active_link()
            .is_local()
            .then(|| skill.places.first().map(|place| place.file.clone()))
            .flatten();
        let reveal = revealed.map(|file| {
            Button::new((REVEAL_PREFIX, id))
                .icon(IconName::Folder)
                .small()
                .variant(ButtonVariant::Ghost)
                .tooltip(REVEAL_TOOLTIP)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.reveal_skill_file(file.clone(), cx);
                }))
        });
        let controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .when(skill.clash, |row| row.child(chip(CLASH_LABEL, pal)))
            .child(chip(scope_label(skill), pal))
            .child(sources)
            .children(reveal)
            .child(
                Switch::new((SWITCH_PREFIX, id))
                    .checked(enabled)
                    .on_click(cx.listener(move |this: &mut Self, on: &bool, _, cx| {
                        this.set_skill_enabled(key.clone(), *on, cx);
                    })),
            )
            .into_any_element();
        let detail =
            (!skill.description.is_empty()).then(|| SharedString::from(skill.description.clone()));
        div()
            .id((ROW_PREFIX, id))
            .rounded(px(RADIUS_XS))
            .hover(|style| style.bg(pal.bg_hover))
            .when(!enabled, |row| row.opacity(DISABLED_OPACITY))
            .child(sheet_row_with(
                Some(icon(kind_mark(skill.key.kind), ICON_SM, pal.text_muted).into_any_element()),
                skill.key.name.clone(),
                detail,
                Some(controls),
                pal,
            ))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_chat::{SkillHarness, SkillScope};
    use krad_domain::{SkillKey, SkillKind};

    #[test]
    fn an_entry_lists_its_scopes_and_every_place_it_comes_from() {
        let place = |harness, scope, file: &str, plugin: Option<&str>| SkillPlace {
            harness,
            scope,
            plugin: plugin.map(str::to_owned),
            file: PathBuf::from(file),
        };
        let skill = LibrarySkill {
            key: SkillKey::new(SkillKind::Skill, "lint"),
            description: String::new(),
            places: vec![
                place(SkillHarness::Shared, SkillScope::Project, "/p/lint", None),
                place(
                    SkillHarness::ClaudeCode,
                    SkillScope::Global,
                    "/h/lint",
                    Some("tools"),
                ),
            ],
            clash: false,
        };
        assert_eq!(scope_label(&skill), "Project · Global");
        assert_eq!(
            places_tooltip(&skill),
            "Shared: /p/lint\nClaude Code (plugin tools): /h/lint"
        );
    }
}
