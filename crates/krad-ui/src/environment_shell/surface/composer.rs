mod usage;

use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, BoxShadow, ClickEvent, Context, ElementId, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{Effort, TerminalId};

use super::super::ask::AskOwner;
use super::super::chat::Composer;
use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::model_picker::{grouped, ModelChoice};
use super::super::EnvironmentShell;
use super::facts::Facts;
use super::mentions::PASTE_KEY;
use super::view::{AgentNow, ROUND_BUTTON};
use super::{SurfaceChat, SurfaceCommand, THINKING_LABEL};
use crate::components::{harness_icon, icon, tooltip_view, IconName};
use crate::theme::{
    palette, row_fill, row_shadow, Palette, ICON_SM, ICON_XS, LINE_SM, LINE_XS, RADIUS_MD,
    RADIUS_XL, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};
use usage::{usage_ring, window_label};

const COMPOSER_RING: f32 = 1.0;
const CONTROL_H: f32 = 24.0;
const SEND_PREFIX: &str = "surface-send";
const MODEL_PREFIX: &str = "surface-model";
const EFFORT_PREFIX: &str = "surface-effort";
const THINKING_PREFIX: &str = "surface-thinking";
const TOOLTIP_SEND: &str = "Send to the agent";
const TOOLTIP_STOP: &str = "Stop the agent";
const TOOLTIP_MODEL: &str = "Model";
const ORCHESTRATOR_LABEL: &str = "Orchestrator";
const ORCHESTRATOR_PREFIX: &str = "surface-orchestrator";
const TOOLTIP_ORCHESTRATOR_OFF: &str =
    "Orchestrator: lead this session, starting and directing Workers";
const TOOLTIP_ORCHESTRATOR_ON: &str = "Leading this session; click to make it a plain chat again";
const TOOLTIP_EFFORT: &str = "Reasoning effort";
const TOOLTIP_THINKING: &str = "Extended thinking";
const MODEL_LABEL: &str = "Model";
const EFFORT_LABEL: &str = "Effort";

impl EnvironmentShell {
    pub(super) fn surface_composer(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mut ring = row_shadow(pal, cx);
        ring.push(
            BoxShadow::new(px(0.0), px(0.0), pal.border.into()).spread_radius(px(COMPOSER_RING)),
        );
        let facts = self.composer_facts(id, chat);
        let now = self.agent_now(id);
        let field = div()
            .relative()
            .flex()
            .flex_row()
            .items_end()
            .w_full()
            .gap(px(SPACE_2))
            .pl(px(SPACE_3))
            .pr(px(SPACE_1_5))
            .py(px(SPACE_1_5))
            .rounded(px(RADIUS_XL))
            .bg(rgb(pal.bg_surface))
            .shadow(ring)
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .capture_key_down(
                cx.listener(move |this: &mut Self, event: &KeyDownEvent, _, cx| {
                    let keystroke = &event.keystroke;
                    if keystroke.key == PASTE_KEY
                        && keystroke.modifiers.secondary()
                        && this.paste_chat_image(id, cx)
                    {
                        cx.stop_propagation();
                    }
                }),
            );
        let field = Self::suggestion_keys(field, Composer::Surface(id), cx)
            .child(
                div().flex_1().min_w_0().py(px(SPACE_1)).child(
                    Textarea::new(&chat.input)
                        .appearance(false)
                        .disabled(chat.asks().blocks_typing()),
                ),
            )
            .child(Self::send_control(id, chat, now, cx))
            .children(self.suggestion_menu(Composer::Surface(id), cx));
        let (mark, mark_color) = harness_icon(chat.harness(), pal);
        let settings = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .px(px(SPACE_1))
            .children(
                chat.structured()
                    .is_some()
                    .then(|| Self::attach_control(id, cx)),
            )
            .children(
                chat.structured()
                    .is_some()
                    .then(|| self.mode_control(AskOwner::Surface(id), cx)),
            )
            .children(
                chat.structured()
                    .is_some()
                    .then(|| self.orchestrator_control(id, cx)),
            )
            .child(div().flex_1())
            .child(icon(mark, ICON_XS, mark_color))
            .children(facts.account.clone().map(|account| {
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(account)
            }))
            .child(Self::model_control(id, &facts, cx))
            .children(Self::effort_control(id, &facts, cx))
            .children(Self::thinking_control(id, &facts, cx))
            .children(facts.context_window.map(|tokens| {
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_subtle))
                    .child(format!("({})", window_label(tokens)))
            }))
            .children(usage_ring(id, &facts, pal));
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(SPACE_1_5))
            .children(Self::attached_tray(id, chat, cx))
            .child(field)
            .child(settings)
            .into_any_element()
    }

    fn orchestrator_control(&self, id: TerminalId, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let on = self.orchestrates(id);
        control_pill((ORCHESTRATOR_PREFIX, id.as_u64()), pal)
            .when(on, |pill| {
                pill.bg(row_fill(pal, cx))
                    .shadow(row_shadow(pal, cx))
                    .text_color(rgb(pal.text_strong))
            })
            .tooltip(move |_, cx| {
                tooltip_view(
                    if on {
                        TOOLTIP_ORCHESTRATOR_ON
                    } else {
                        TOOLTIP_ORCHESTRATOR_OFF
                    }
                    .into(),
                    cx,
                )
            })
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.toggle_orchestrator(id, cx);
            }))
            .child(icon(
                IconName::Crown,
                ICON_XS,
                if on { pal.text_strong } else { pal.text_muted },
            ))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(ORCHESTRATOR_LABEL),
            )
            .into_any_element()
    }

    fn model_control(id: TerminalId, facts: &Facts, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let label = facts
            .model
            .clone()
            .unwrap_or_else(|| MODEL_LABEL.to_owned());
        let groups = std::rc::Rc::new(grouped(facts.models.iter().map(|pick| {
            (
                pick.harness,
                ModelChoice {
                    id: pick.id.clone(),
                    label: pick.label.clone(),
                    current: pick.current,
                    section: pick.source.clone(),
                },
            )
        })));
        control_pill((MODEL_PREFIX, id.as_u64()), pal)
            .tooltip(|_, cx| tooltip_view(TOOLTIP_MODEL.into(), cx))
            .on_click(
                cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                    let at = event.position();
                    let origin = (at.x.as_f32(), at.y.as_f32());
                    this.open_model_picker(id, origin, groups.to_vec(), cx);
                }),
            )
            .child(div().font_weight(FontWeight::MEDIUM).child(label))
            .child(icon(IconName::ChevronDown, ICON_XS, pal.text_subtle))
            .into_any_element()
    }

    fn effort_control(id: TerminalId, facts: &Facts, cx: &mut Context<Self>) -> Option<AnyElement> {
        if facts.efforts.is_empty() {
            return None;
        }
        let pal = palette(cx);
        let label = facts.effort.map_or(EFFORT_LABEL, Effort::label);
        let entries: Vec<MenuEntry> = std::iter::once(MenuEntry::Header(EFFORT_LABEL.to_owned()))
            .chain(facts.efforts.iter().map(|effort| {
                MenuEntry::checked(
                    effort.label(),
                    facts.effort == Some(*effort),
                    MenuAction::SetEffort(id, *effort),
                )
            }))
            .collect();
        let entries = std::rc::Rc::new(entries);
        Some(
            control_pill((EFFORT_PREFIX, id.as_u64()), pal)
                .tooltip(|_, cx| tooltip_view(TOOLTIP_EFFORT.into(), cx))
                .on_click(
                    cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                        this.open_menu_above(event, entries.to_vec(), cx);
                    }),
                )
                .child(icon(IconName::Flash, ICON_XS, pal.text_muted))
                .child(label)
                .child(icon(IconName::ChevronDown, ICON_XS, pal.text_subtle))
                .into_any_element(),
        )
    }

    fn open_menu_above(
        &mut self,
        event: &ClickEvent,
        entries: Vec<MenuEntry>,
        cx: &mut Context<Self>,
    ) {
        let position = event.position();
        self.open_menu(
            (position.x.as_f32(), position.y.as_f32()),
            MenuPlacement::Above,
            entries,
            cx,
        );
    }

    fn thinking_control(
        id: TerminalId,
        facts: &Facts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let on = facts.thinking?;
        let pal = palette(cx);
        Some(
            control_pill((THINKING_PREFIX, id.as_u64()), pal)
                .when(on, |pill| {
                    pill.bg(row_fill(pal, cx))
                        .shadow(row_shadow(pal, cx))
                        .text_color(rgb(pal.text_strong))
                })
                .tooltip(|_, cx| tooltip_view(TOOLTIP_THINKING.into(), cx))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.command_surface(id, SurfaceCommand::ToggleThinking, cx);
                }))
                .child(icon(
                    IconName::Magic,
                    ICON_XS,
                    if on { pal.accent } else { pal.text_muted },
                ))
                .child(THINKING_LABEL)
                .into_any_element(),
        )
    }

    fn send_control(
        id: TerminalId,
        chat: &SurfaceChat,
        now: AgentNow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let empty = chat.input.read(cx).value().trim().is_empty() && chat.attached.is_empty();
        let stops = empty && matches!(now, AgentNow::Working | AgentNow::Waiting);
        let (glyph, tooltip, fill, color): (IconName, &'static str, gpui_kit::Hsla, u32) = if stops
        {
            (
                IconName::Stop,
                TOOLTIP_STOP,
                rgb(pal.text_strong).into(),
                pal.bg_surface,
            )
        } else if empty {
            (
                IconName::Send,
                TOOLTIP_SEND,
                pal.bg_hover.into(),
                pal.text_muted,
            )
        } else {
            (
                IconName::Send,
                TOOLTIP_SEND,
                rgb(pal.accent).into(),
                pal.on_accent,
            )
        };
        div()
            .id((SEND_PREFIX, id.as_u64()))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .ml(px(SPACE_1))
            .size(px(ROUND_BUTTON))
            .rounded_full()
            .bg(fill)
            .when(stops || !empty, Styled::cursor_pointer)
            .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                if stops {
                    this.command_surface(id, SurfaceCommand::Stop, cx);
                } else {
                    this.send_on_surface(id, window, cx);
                }
            }))
            .child(icon(glyph, ICON_SM, color))
            .into_any_element()
    }
}

fn control_pill(id: impl Into<ElementId>, pal: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(SPACE_1))
        .h(px(CONTROL_H))
        .px(px(SPACE_2))
        .rounded(px(RADIUS_MD))
        .cursor_pointer()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_default))
        .hover(|style| style.bg(pal.bg_hover))
}
