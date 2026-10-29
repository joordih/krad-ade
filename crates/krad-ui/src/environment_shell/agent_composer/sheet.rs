use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, BoxShadow, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window,
};

use super::super::dialog_parts::{field_label, sheet_overlay, sheet_panel, sheet_title};
use super::super::EnvironmentShell;
use super::AgentComposer;
use crate::components::{harness_icon, icon, Button, ButtonVariant, IconName};
use crate::theme::{
    palette, row_fill, row_shadow, surface_fill, Palette, ICON_SM, LINE_XS, SPACE_1, SPACE_1_5,
    SPACE_2, SPACE_3, SPACE_5, TEXT_SM, TEXT_XS,
};

const SHEET_W: f32 = 560.0;
const CHIP_H: f32 = 26.0;
const CHIP_PAD_X: f32 = 10.0;
const CHIP_RING: f32 = 1.5;
const BTN_CLOSE_COMPOSER: &str = "composer-close";
const TITLE: &str = "New agent";
const AGENT_LABEL: &str = "Agent";
const PROMPT_LABEL: &str = "Prompt";
const WHERE_LABEL: &str = "Run in";
const NEW_WORKTREE_LABEL: &str = "New worktree";
const NO_GROUP_TEXT: &str = "No group: the agent starts in a plain terminal";
const START_LABEL: &str = "Start agent";
const CANCEL_LABEL: &str = "Cancel";
const BTN_START: &str = "composer-start";
const BTN_CANCEL: &str = "composer-cancel";
const AGENT_CHIP_PREFIX: &str = "composer-agent";
const GROUP_CHIP_PREFIX: &str = "composer-group";
const WORKTREE_CHIP: &str = "composer-worktree";

fn chip(selected: bool, pal: &Palette, cx: &gpui_kit::App) -> gpui_kit::Div {
    let mut ring = row_shadow(pal, cx);
    ring.push(
        BoxShadow::new(px(0.0), px(0.0), rgb(pal.accent).into()).spread_radius(px(CHIP_RING)),
    );
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1_5))
        .h(px(CHIP_H))
        .px(px(CHIP_PAD_X))
        .rounded_full()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(if selected {
            pal.text_strong
        } else {
            pal.text_default
        }))
        .cursor_pointer()
        .when(selected, |chip| chip.bg(row_fill(pal, cx)).shadow(ring))
        .when(!selected, |chip| chip.bg(surface_fill(pal.bg_panel, cx)))
}

fn close(this: &mut EnvironmentShell, window: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_agent_composer(window, cx);
}

impl EnvironmentShell {
    fn composer_agents(&self, composer: &AgentComposer, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pal = palette(cx);
        self.installed_agents()
            .iter()
            .copied()
            .filter(|preset| preset.is_agent())
            .map(|preset| {
                let (mark, mark_color) = harness_icon(preset, pal);
                chip(composer.preset == preset, pal, cx)
                    .id((AGENT_CHIP_PREFIX, preset as u64))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        if let Some(composer) = &mut this.agent_composer {
                            composer.preset = preset;
                        }
                        cx.notify();
                    }))
                    .child(icon(mark, ICON_SM, mark_color))
                    .child(preset.display_name())
                    .into_any_element()
            })
            .collect()
    }

    fn composer_targets(
        &self,
        composer: &AgentComposer,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, AnyElement) {
        let pal = palette(cx);
        let groups: Vec<AnyElement> = self
            .envs
            .get(&self.active)
            .map_or_default(|live| live.data.groups.clone())
            .into_iter()
            .map(|group| {
                let id = group.id;
                chip(composer.group == Some(id), pal, cx)
                    .id((GROUP_CHIP_PREFIX, id.as_u64()))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        if let Some(composer) = &mut this.agent_composer {
                            composer.group = Some(id);
                        }
                        cx.notify();
                    }))
                    .child(icon(IconName::Folder, ICON_SM, pal.text_subtle))
                    .child(group.name)
                    .into_any_element()
            })
            .collect();
        let worktree_toggle = chip(composer.new_worktree, pal, cx)
            .id(WORKTREE_CHIP)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                if let Some(composer) = &mut this.agent_composer {
                    composer.new_worktree = !composer.new_worktree;
                }
                cx.notify();
            }))
            .child(icon(IconName::GitBranch, ICON_SM, pal.text_subtle))
            .child(NEW_WORKTREE_LABEL)
            .into_any_element();
        (groups, worktree_toggle)
    }

    fn composer_actions(cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .py(px(SPACE_3))
            .px(px(SPACE_5))
            .border_t_1()
            .border_color(pal.divider)
            .child(div().flex_1())
            .child(
                Button::new(BTN_CANCEL)
                    .label(CANCEL_LABEL)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.close_agent_composer(window, cx);
                    })),
            )
            .child(
                Button::new(BTN_START)
                    .label(START_LABEL)
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.start_composed_agent(window, cx);
                    })),
            )
            .into_any_element()
    }
    pub(in crate::environment_shell) fn agent_composer_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let composer = self.agent_composer.as_ref()?;
        let pal = palette(cx);
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(TITLE, None, BTN_CLOSE_COMPOSER, close, cx))
            .child(self.composer_fields(composer, cx))
            .child(Self::composer_actions(cx));
        Some(sheet_overlay(sheet, close, cx))
    }

    fn composer_fields(&self, composer: &AgentComposer, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let agents = self.composer_agents(composer, cx);
        let (groups, worktree_toggle) = self.composer_targets(composer, cx);
        let has_groups = !groups.is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .px(px(SPACE_5))
            .pb(px(SPACE_5))
            .child(field_label(AGENT_LABEL, pal))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(SPACE_1))
                    .children(agents),
            )
            .child(field_label(PROMPT_LABEL, pal))
            .child(Textarea::new(&composer.prompt))
            .child(field_label(WHERE_LABEL, pal))
            .when(has_groups, |sheet| {
                sheet.child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(SPACE_1))
                        .children(groups)
                        .child(worktree_toggle),
                )
            })
            .when(!has_groups, |sheet| {
                sheet.child(
                    div()
                        .text_size(px(TEXT_SM))
                        .text_color(rgb(pal.text_subtle))
                        .child(NO_GROUP_TEXT),
                )
            })
            .when(composer.new_worktree, |sheet| {
                sheet.child(Input::new(&composer.worktree_name))
            })
            .into_any_element()
    }
}
