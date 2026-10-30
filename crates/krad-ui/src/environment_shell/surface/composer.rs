use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    canvas, div, point, px, rgb, AnyElement, Bounds, BoxShadow, ClickEvent, Context, ElementId,
    FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, PathBuilder,
    Pixels, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{Effort, Route, TerminalId};
use std::f32::consts::{FRAC_PI_2, TAU};

use super::super::ask::AskOwner;
use super::super::chat::Composer;
use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::model_picker::{grouped, ModelChoice};
use super::super::EnvironmentShell;
use super::facts::Facts;
use super::view::{AgentNow, ROUND_BUTTON};
use super::{SurfaceChat, SurfaceCommand, THINKING_LABEL};
use crate::components::{harness_icon, icon, tooltip_view, IconName};
use crate::theme::{
    palette, row_fill, row_shadow, Palette, ICON_SM, ICON_XS, LINE_SM, LINE_XS, RADIUS_MD,
    RADIUS_XL, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const COMPOSER_RING: f32 = 1.0;
const CONTROL_H: f32 = 24.0;
const WARN_PERCENT: u16 = 80;
const SEND_PREFIX: &str = "surface-send";
const MODEL_PREFIX: &str = "surface-model";
const EFFORT_PREFIX: &str = "surface-effort";
const THINKING_PREFIX: &str = "surface-thinking";
const USAGE_PREFIX: &str = "surface-usage";
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
const GATEWAY_MARK: &str = "via Gateway";
const EFFORT_LABEL: &str = "Effort";
const CONTEXT_LABEL: &str = "Context";
const FIVE_HOUR_LABEL: &str = "5h";
const SEVEN_DAY_LABEL: &str = "7d";
const PERCENT_SIGN: &str = "%";
const USAGE_SEPARATOR: &str = " · ";
const MILLION: u64 = 1_000_000;
const THOUSAND: u64 = 1_000;
const MILLION_SUFFIX: &str = "M";
const THOUSAND_SUFFIX: &str = "K";
const FULL_PERCENT: u16 = 100;
const RING_SIZE: f32 = 16.0;
const RING_STROKE: f32 = 2.0;
const RING_SEGMENTS: u16 = 48;

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
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
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
        let (mark, mark_color) = match facts.route {
            Route::Direct => harness_icon(chat.harness(), pal),
            Route::Gateway => (IconName::Flow, pal.accent),
        };
        let account = match facts.route {
            Route::Direct => facts.account.clone(),
            Route::Gateway => Some(GATEWAY_MARK.to_owned()),
        };
        let settings = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .px(px(SPACE_1))
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
            .children(account.map(|account| {
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
        let groups = std::rc::Rc::new(grouped(
            facts.lanes.iter().copied(),
            facts.models.iter().map(|pick| {
                (
                    pick.lane,
                    ModelChoice {
                        id: pick.id.clone(),
                        label: pick.label.clone(),
                        current: pick.current,
                        section: pick.source.clone(),
                    },
                )
            }),
        ));
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
        let empty = chat.input.read(cx).value().trim().is_empty();
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

fn window_label(tokens: u64) -> String {
    if tokens >= MILLION && tokens.is_multiple_of(MILLION) {
        format!("{}{MILLION_SUFFIX}", tokens / MILLION)
    } else if tokens >= THOUSAND {
        format!("{}{THOUSAND_SUFFIX}", tokens / THOUSAND)
    } else {
        tokens.to_string()
    }
}

fn usage_text(facts: &Facts) -> Option<String> {
    let parts: Vec<String> = [
        (CONTEXT_LABEL, facts.context),
        (FIVE_HOUR_LABEL, facts.usage.five_hour),
        (SEVEN_DAY_LABEL, facts.usage.seven_day),
    ]
    .into_iter()
    .filter_map(|(label, used)| Some(format!("{label} {}{PERCENT_SIGN}", used?)))
    .collect();
    (!parts.is_empty()).then(|| parts.join(USAGE_SEPARATOR))
}

fn usage_ring(id: TerminalId, facts: &Facts, pal: &Palette) -> Option<AnyElement> {
    let text = SharedString::from(usage_text(facts)?);
    let used = facts.context.unwrap_or_default().min(FULL_PERCENT);
    let warns = [facts.context, facts.usage.five_hour, facts.usage.seven_day]
        .into_iter()
        .flatten()
        .any(|used| used >= WARN_PERCENT);
    let fill: Hsla = rgb(if warns {
        pal.attention_text
    } else {
        pal.text_muted
    })
    .into();
    let track: Hsla = pal.divider.into();
    let fraction = f32::from(used) / f32::from(FULL_PERCENT);
    Some(
        div()
            .id((USAGE_PREFIX, id.as_u64()))
            .flex_none()
            .size(px(RING_SIZE))
            .ml(px(SPACE_1))
            .tooltip(move |_, cx| tooltip_view(text.clone(), cx))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _| {
                        paint_ring(bounds, 1.0, track, window);
                        paint_ring(bounds, fraction, fill, window);
                    },
                )
                .size_full(),
            )
            .into_any_element(),
    )
}

fn paint_ring(bounds: Bounds<Pixels>, fraction: f32, color: Hsla, window: &mut Window) {
    if fraction <= 0.0 {
        return;
    }
    let size = bounds.size.width.as_f32().min(bounds.size.height.as_f32());
    let radius = (size - RING_STROKE) / 2.0;
    let (cx, cy) = (
        bounds.origin.x.as_f32() + size / 2.0,
        bounds.origin.y.as_f32() + size / 2.0,
    );
    let at = |turn: f32| {
        let angle = turn * TAU - FRAC_PI_2;
        point(px(cx + radius * angle.cos()), px(cy + radius * angle.sin()))
    };
    let mut ring = PathBuilder::stroke(px(RING_STROKE));
    ring.move_to(at(0.0));
    for step in 1..=RING_SEGMENTS {
        ring.line_to(at(fraction * f32::from(step) / f32::from(RING_SEGMENTS)));
    }
    if let Ok(path) = ring.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_context_window_reads_in_millions_or_thousands() {
        assert_eq!(window_label(1_000_000), "1M");
        assert_eq!(window_label(200_000), "200K");
        assert_eq!(window_label(1_500_000), "1500K");
        assert_eq!(window_label(512), "512");
    }
}
