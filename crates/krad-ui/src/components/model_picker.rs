use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    deferred, div, px, rgb, AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled,
    Window,
};
use krad_domain::{AgentPreset, ModelId};
use std::rc::Rc;

use super::icon::{harness_icon, icon, IconName};
use crate::theme::{
    palette, popover_fill, popover_shadow, row_fill, row_shadow, ICON_SM, ICON_XS, LINE_XS,
    NAV_ROW_H, RADIUS_MD, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_XS,
};

const TRIGGER_RADIUS: f32 = 8.0;
const LIST_PRIORITY: usize = 1;
const LIST_W: f32 = 288.0;
const LIST_OFFSET: f32 = 6.0;
const MODEL_INDENT: f32 = 26.0;
const FOOTER_H: f32 = 30.0;
const DIVIDER_H: f32 = 1.0;
const MISSING_NOTE: &str = "Not found on PATH";
const FOOTER_TEXT: &str = "Add models under harness_models in settings.json";
const ROW_PREFIX: &str = "model-option";
const DEFAULT_MODEL_LABEL: &str = "Default model";

type PickHandler = Rc<dyn Fn(AgentPreset, Option<ModelId>, &mut Window, &mut App)>;
type ToggleHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Debug)]
pub(crate) struct ModelChoice {
    pub(crate) preset: AgentPreset,
    pub(crate) models: Vec<ModelId>,
    pub(crate) available: bool,
}

#[derive(IntoElement)]
pub(crate) struct ModelPicker {
    id: ElementId,
    preset: AgentPreset,
    model: Option<ModelId>,
    choices: Vec<ModelChoice>,
    open: bool,
    opens_up: bool,
    on_toggle: Option<ToggleHandler>,
    on_pick: Option<PickHandler>,
}

impl ModelPicker {
    pub(crate) fn new(
        id: impl Into<ElementId>,
        preset: AgentPreset,
        model: Option<ModelId>,
        choices: Vec<ModelChoice>,
    ) -> Self {
        Self {
            id: id.into(),
            preset,
            model,
            choices,
            open: false,
            opens_up: false,
            on_toggle: None,
            on_pick: None,
        }
    }

    pub(crate) const fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub(crate) const fn opens_up(mut self) -> Self {
        self.opens_up = true;
        self
    }

    pub(crate) fn on_toggle(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }

    pub(crate) fn on_pick(
        mut self,
        handler: impl Fn(AgentPreset, Option<ModelId>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_pick = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ModelPicker {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let (mark, mark_color) = harness_icon(self.preset, pal);
        let on_toggle = self.on_toggle;
        let trigger = div()
            .id(self.id.clone())
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(NAV_ROW_H))
            .px(px(SPACE_2))
            .rounded(px(TRIGGER_RADIUS))
            .bg(pal.bg_hover)
            .cursor_pointer()
            .whitespace_nowrap()
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(pal.text_strong))
            .child(icon(mark, ICON_SM, mark_color))
            .child(self.preset.display_name())
            .children(self.model.as_ref().map(|model| {
                div()
                    .font_family(mono.clone())
                    .font_weight(FontWeight::NORMAL)
                    .text_color(rgb(pal.text_muted))
                    .child(model.to_string())
            }))
            .child(icon(IconName::ChevronDown, ICON_XS, pal.text_muted))
            .when_some(on_toggle, |trigger, handler| {
                trigger.on_click(move |event, window, cx| handler(event, window, cx))
            });
        let list = self.open.then(|| {
            let groups: Vec<AnyElement> = self
                .choices
                .iter()
                .map(|choice| {
                    model_group(
                        choice,
                        (self.preset, self.model.as_ref()),
                        self.on_pick.as_ref(),
                        &mono,
                        cx,
                    )
                })
                .collect();
            div()
                .absolute()
                .left_0()
                .when(self.opens_up, |list| {
                    list.bottom(px(NAV_ROW_H + LIST_OFFSET))
                })
                .when(!self.opens_up, |list| list.top(px(NAV_ROW_H + LIST_OFFSET)))
                .w(px(LIST_W))
                .flex()
                .flex_col()
                .p(px(SPACE_1))
                .rounded(px(RADIUS_MD))
                .bg(popover_fill(pal, cx))
                .border_1()
                .border_color(pal.border)
                .shadow(popover_shadow(pal, cx))
                .occlude()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .children(groups)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_1_5))
                        .h(px(FOOTER_H))
                        .mt(px(SPACE_1))
                        .px(px(SPACE_2))
                        .border_t(px(DIVIDER_H))
                        .border_color(pal.divider)
                        .text_color(rgb(pal.text_muted))
                        .child(icon(IconName::Settings, ICON_XS, pal.text_muted))
                        .child(FOOTER_TEXT),
                )
        });
        div()
            .relative()
            .flex_none()
            .child(trigger)
            .children(list.map(|list| deferred(list).with_priority(LIST_PRIORITY)))
    }
}

fn model_group(
    choice: &ModelChoice,
    current: (AgentPreset, Option<&ModelId>),
    on_pick: Option<&PickHandler>,
    mono: &SharedString,
    cx: &App,
) -> AnyElement {
    let pal = palette(cx);
    let (mark, mark_color) = harness_icon(choice.preset, pal);
    let picks: Vec<Option<&ModelId>> = if choice.models.is_empty() {
        vec![None]
    } else {
        choice.models.iter().map(Some).collect()
    };
    let rows = picks.into_iter().enumerate().map(|(index, model)| {
        let selected = current.0 == choice.preset && current.1 == model;
        let preset = choice.preset;
        let picked = model.cloned();
        let label = model.map_or_else(|| DEFAULT_MODEL_LABEL.to_owned(), ToString::to_string);
        let handler = on_pick.cloned();
        div()
            .id(SharedString::from(format!(
                "{ROW_PREFIX}-{preset:?}-{index}"
            )))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(NAV_ROW_H))
            .pl(px(MODEL_INDENT))
            .pr(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .font_family(mono.clone())
            .text_color(rgb(if choice.available {
                pal.text_strong
            } else {
                pal.text_disabled
            }))
            .when(selected, |row| {
                row.bg(row_fill(pal, cx)).shadow(row_shadow(pal, cx))
            })
            .when(choice.available && !selected, |row| {
                row.cursor_pointer().hover(|style| style.bg(pal.bg_hover))
            })
            .when_some(handler.filter(|_| choice.available), |row, handler| {
                row.on_click(move |_, window, cx| handler(preset, picked.clone(), window, cx))
            })
            .child(div().flex_1().child(label))
            .when(selected, |row| {
                row.child(icon(IconName::Tick, ICON_SM, pal.text_strong))
            })
    });
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_1_5))
                .h(px(NAV_ROW_H))
                .pt(px(SPACE_1))
                .px(px(SPACE_2))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(pal.text_subtle))
                .child(icon(mark, ICON_XS, mark_color))
                .child(choice.preset.display_name())
                .child(div().flex_1())
                .when(!choice.available, |heading| {
                    heading.child(
                        div()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(rgb(pal.text_muted))
                            .child(MISSING_NOTE),
                    )
                }),
        )
        .children(rows)
        .into_any_element()
}
