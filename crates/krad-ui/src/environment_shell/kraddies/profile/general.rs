use gpui_kit::{
    div, px, AnyElement, Context, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{Effort, KraddieId, KraddieSettings, PermissionMode, Switch};

use super::super::super::ask::{mode_label, OFFERED_MODES};
use super::super::super::EnvironmentShell;
use super::super::layout::{place, CardSpan, PageLayout};
use super::super::segmented::sliding_width;
use super::super::segmented::{sliding_track, Sliding};
use super::super::tile::{button_w, grid, setting_row, tile, tile_inner, SettingRow, Tile};
use crate::components::{Button, ButtonVariant, IconName, Stretch};
use crate::kraddie_avatar::kraddie_avatar;
use crate::theme::{palette, ICON_BUTTON_SM, SPACE_2, SPACE_3};
use gpui_kit::component::switch::Switch as SwitchControl;

const AVATAR_SIDE: f32 = 64.0;

struct Choice<T: 'static> {
    title: &'static str,
    prefix: &'static str,
    current: Option<T>,
    choices: &'static [T],
    label: fn(T) -> &'static str,
    apply: fn(&mut KraddieSettings, Option<T>),
}
const TUNING: [CardSpan; 6] = [CardSpan::One; 6];
const TILE_PREFIX: &str = "kraddie-tuning";
const LOOK_TITLE: &str = "Look";
const LOOK_NOTE: &str = "Pixel by pixel, one face per mood";
const STUDIO_BTN: &str = "kraddie-general-studio";
const STUDIO_LABEL: &str = "Open the studio";
const BRAIN_TITLE: &str = "Brain";
const BRAIN_NOTE: &str = "Which agent thinks for it, and how freely";
const AGENT_LABEL: &str = "Agent";
const DEFAULT_MODEL: &str = "its default model";
const AGENT_BTN: &str = "kraddie-general-agent";
const AGENT_EDIT: &str = "Change";
const EFFORT_LABEL: &str = "Thinking effort";
const EFFORT_PREFIX: &str = "kraddie-effort";
const MODE_LABEL: &str = "Permissions";
const MODE_PREFIX: &str = "kraddie-mode";
const DEFAULT_CHOICE: &str = "Default";
const REACH_TITLE: &str = "Reach";
const REACH_NOTE: &str = "Repositories it may read besides its own";
const ADD_PROJECT: &str = "kraddie-add-project";
const ADD_PROJECT_LABEL: &str = "Lend it this group";
const NO_PROJECT_GROUP: &str = "Pick a group first; its repository is the one lent";
const REMOVE_PROJECT_PREFIX: &str = "kraddie-remove-project";
const REMOVE_TOOLTIP: &str = "Take it back";
const WORKS_IN: &str = "Home";
const NO_PROJECTS: &str = "Nothing lent yet";

impl EnvironmentShell {
    pub(super) fn edit_kraddie_settings(
        &mut self,
        id: &KraddieId,
        edit: impl FnOnce(&mut KraddieSettings),
        cx: &mut Context<Self>,
    ) {
        let Some(mut kraddie) = self.kraddies.entry(id).map(|entry| entry.kraddie.clone()) else {
            return;
        };
        edit(&mut kraddie.settings);
        self.save_kraddie(kraddie, Some(id), cx);
    }

    pub(super) fn switch_control(
        prefix: &'static str,
        current: Switch,
        id: &KraddieId,
        apply: fn(&mut KraddieSettings, Switch),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = id.clone();
        SwitchControl::new(SharedString::from(format!("{prefix}-{id}")))
            .checked(current.is_on())
            .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
                this.edit_kraddie_settings(
                    &target,
                    |settings| apply(settings, current.toggled()),
                    cx,
                );
            }))
            .into_any_element()
    }

    fn choice_row<T: Copy + PartialEq + 'static>(
        &self,
        choice: &Choice<T>,
        id: &KraddieId,
        card_inner: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let options: Vec<Option<T>> = std::iter::once(None)
            .chain(choice.choices.iter().copied().map(Some))
            .collect();
        let labels: Vec<&'static str> = options
            .iter()
            .map(|option| option.map_or(DEFAULT_CHOICE, choice.label))
            .collect();
        let menu = choice.prefix;
        let row = SettingRow::new(choice.title, sliding_width(&labels, cx), card_inner);
        let container = row.room(cx);
        let sliding = Sliding {
            id: menu,
            labels: &labels,
            short: &labels,
            chosen: options
                .iter()
                .position(|option| *option == choice.current)
                .unwrap_or_default(),
            container,
            stretch: Stretch::Natural,
            menu_open: self.kraddies.profile.open_menu == Some(menu),
        };
        let apply = choice.apply;
        let control = sliding_track(
            &sliding,
            pal,
            cx,
            |index, segment| {
                let picked = options.get(index).copied().flatten();
                let target = id.clone();
                segment.on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.kraddies.profile.open_menu = None;
                    this.edit_kraddie_settings(&target, |settings| apply(settings, picked), cx);
                }))
            },
            |button| {
                button.on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.kraddies.profile.toggle_menu(menu);
                    cx.notify();
                }))
            },
        );
        setting_row(row, Some(control), pal, cx)
    }

    pub(super) fn tuning_grid(
        &self,
        id: &KraddieId,
        layout: &PageLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let spans = place(&TUNING, layout.tuning_columns);
        let key = id.to_string();
        let inner = |index: usize| tile_inner(layout.span_w(spans[index], layout.tuning_columns));
        let cards: [(&'static str, Option<&'static str>, AnyElement); 6] = [
            (LOOK_TITLE, Some(LOOK_NOTE), self.look_part(id, cx)),
            (
                BRAIN_TITLE,
                Some(BRAIN_NOTE),
                self.brain_part(id, inner(1), cx),
            ),
            (
                REACH_TITLE,
                Some(REACH_NOTE),
                self.projects_part(id, inner(2), cx),
            ),
            self.sessions_part(id, inner(3), cx),
            self.rounds_part(id, inner(4), cx),
            self.conversation_part(id, inner(5), cx),
        ];
        let tiles = cards
            .into_iter()
            .enumerate()
            .map(|(index, (title, note, body))| {
                tile(
                    Tile {
                        id: SharedString::from(format!("{TILE_PREFIX}-{key}-{title}")),
                        index,
                        width: layout.span_w(spans[index], layout.tuning_columns),
                        title,
                        note: note.map(SharedString::from),
                        accessory: None,
                        body,
                    },
                    pal,
                )
            })
            .collect();
        grid(layout.gutter, tiles).into_any_element()
    }

    fn look_part(&self, id: &KraddieId, cx: &mut Context<Self>) -> AnyElement {
        let Some(entry) = self.kraddies.entry(id) else {
            return div().into_any_element();
        };
        let sprite = entry.sprite(self.kraddie_mood(entry));
        let studio = id.clone();
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_3))
            .child(kraddie_avatar(sprite, AVATAR_SIDE, self.kraddies.frame()))
            .child(
                Button::new(STUDIO_BTN)
                    .label(STUDIO_LABEL)
                    .icon(IconName::Pencil)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.open_kraddie_studio(&studio, cx);
                    })),
            )
            .into_any_element()
    }

    fn brain_part(&self, id: &KraddieId, container: f32, cx: &mut Context<Self>) -> AnyElement {
        let Some(entry) = self.kraddies.entry(id) else {
            return div().into_any_element();
        };
        let pal = palette(cx);
        let settings = &entry.kraddie.settings;
        let agent = id.clone();
        let harness = self.settings.acp.harness_name(entry.kraddie.harness);
        let model = entry
            .kraddie
            .model
            .as_ref()
            .map_or_else(|| DEFAULT_MODEL.to_owned(), ToString::to_string);
        let agent_row = SettingRow::new(AGENT_LABEL, button_w(AGENT_EDIT, cx), container)
            .detail(format!("{harness} · {model}"));
        let mut lines = vec![setting_row(
            agent_row,
            Some(
                Button::new(AGENT_BTN)
                    .label(AGENT_EDIT)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.open_kraddie_editor(Some(&agent), window, cx);
                    }))
                    .into_any_element(),
            ),
            pal,
            cx,
        )];
        let efforts = entry.kraddie.harness.offered_efforts();
        if !efforts.is_empty() {
            let choice = Choice {
                title: EFFORT_LABEL,
                prefix: EFFORT_PREFIX,
                current: settings.effort,
                choices: efforts,
                label: Effort::label,
                apply: |settings, effort| settings.effort = effort,
            };
            lines.push(self.choice_row(&choice, id, container, cx));
        }
        let choice = Choice {
            title: MODE_LABEL,
            prefix: MODE_PREFIX,
            current: settings.permissions,
            choices: &OFFERED_MODES,
            label: mode_label,
            apply: |settings, mode: Option<PermissionMode>| settings.permissions = mode,
        };
        lines.push(self.choice_row(&choice, id, container, cx));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .children(lines)
            .into_any_element()
    }

    fn projects_part(&self, id: &KraddieId, card_inner: f32, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Some(entry) = self.kraddies.entry(id) else {
            return div().into_any_element();
        };
        let home = entry.kraddie.workspace.as_ref().map(|workspace| {
            setting_row(
                SettingRow::new(workspace.repo.display().to_string(), 0.0, card_inner)
                    .detail(WORKS_IN),
                None,
                pal,
                cx,
            )
        });
        let projects =
            entry
                .kraddie
                .settings
                .projects
                .iter()
                .enumerate()
                .map(|(index, project)| {
                    let target = id.clone();
                    setting_row(
                        SettingRow::new(
                            project.repo.display().to_string(),
                            ICON_BUTTON_SM,
                            card_inner,
                        )
                        .detail(project.environment.as_str().to_owned()),
                        Some(
                            Button::new((REMOVE_PROJECT_PREFIX, index))
                                .icon(IconName::Minus)
                                .small()
                                .tooltip(REMOVE_TOOLTIP)
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.edit_kraddie_settings(
                                        &target,
                                        |settings| {
                                            if index < settings.projects.len() {
                                                settings.projects.remove(index);
                                            }
                                        },
                                        cx,
                                    );
                                }))
                                .into_any_element(),
                        ),
                        pal,
                        cx,
                    )
                });
        let mut rows: Vec<AnyElement> = home.into_iter().chain(projects).collect();
        if entry.kraddie.settings.projects.is_empty() {
            rows.push(setting_row(
                SettingRow::new(NO_PROJECTS, 0.0, card_inner),
                None,
                pal,
                cx,
            ));
        }
        let target = id.clone();
        let add = Button::new(ADD_PROJECT)
            .label(ADD_PROJECT_LABEL)
            .icon(IconName::FolderAdd)
            .small()
            .variant(ButtonVariant::Secondary)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                let Some(project) = this.active_kraddie_workspace() else {
                    this.toast(NO_PROJECT_GROUP, cx);
                    return;
                };
                this.edit_kraddie_settings(
                    &target,
                    |settings| {
                        settings.add_project(project);
                    },
                    cx,
                );
            }));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .children(rows)
            .child(div().flex().flex_row().child(add))
            .into_any_element()
    }
}
