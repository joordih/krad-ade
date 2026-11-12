use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, BoxShadow, ClickEvent, Context, ElementId, InteractiveElement,
    IntoElement, MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    Window,
};
use krad_domain::TerminalId;

use super::super::super::model_picker::{grouped, ModelChoice, PickFor};
use super::super::facts::Facts;
use super::super::{SurfaceChat, SurfaceCommand};
use super::usage::{context_percent, usage_ring};
use super::EnvironmentShell;
use crate::components::{harness_icon, icon, tooltip_view, IconName};
use crate::theme::{
    level_fill, palette, row_fill, row_shadow, tinted, Palette, SurfaceLevel, ICON_SM, ICON_XS,
    LINE_SM, LINE_XS, SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const TRAY_RADIUS: f32 = 20.0;
const CARD_RADIUS: f32 = 16.0;
const CARD_SHADE_ALPHA: u32 = 0x8c;
const TRAY_PAD: f32 = 5.0;
const TRAY_RING: f32 = 1.0;
const PILL_H: f32 = 28.0;
const PILL_RADIUS: f32 = 14.0;
const SWITCH_SIZE: f32 = 24.0;
const SWITCH_GROUP_PAD: f32 = 2.0;
const TOOL_SIZE: f32 = 30.0;
const TOOL_RADIUS: f32 = 9.0;
const TEXT_MIN_H: f32 = 40.0;
const MENTION_KEY: &str = "@";
const SKILL_KEY: &str = "/";
const MODEL_PREFIX: &str = "tray-model";
const THINKING_PREFIX: &str = "tray-thinking";
const ORCHESTRATOR_PREFIX: &str = "tray-orchestrator";
const MENTION_PREFIX: &str = "tray-mention";
const SKILL_PREFIX: &str = "tray-skill";
const EFFORT_SEPARATOR: &str = "·";
const TOOLTIP_MODEL: &str = "Model and effort";
const TOOLTIP_MENTION: &str = "Mention a file";
const TOOLTIP_SKILL: &str = "Use a skill";
const TOOLTIP_THINKING_ON: &str = "Extended thinking is on";
const TOOLTIP_THINKING_OFF: &str = "Extended thinking is off";
const TOOLTIP_ORCHESTRATOR_OFF: &str = "Lead this session, starting and directing Workers";
const TOOLTIP_ORCHESTRATOR_ON: &str = "Leading this session; click to make it a plain chat again";
const MODEL_FALLBACK: &str = "Model";
const PERCENT_SIGN: &str = "%";

impl EnvironmentShell {
    pub(super) fn tray_composer(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let facts = self.composer_facts(id, chat);
        let card = self.tray_card(id, chat, cx);
        let foot = self.tray_foot(id, chat, &facts, cx);
        let ring = BoxShadow::new(px(0.0), px(0.0), pal.border.into()).spread_radius(px(TRAY_RING));
        div()
            .flex()
            .flex_col()
            .w_full()
            .p(px(TRAY_PAD))
            .rounded(px(TRAY_RADIUS))
            .bg(level_fill(SurfaceLevel::Inset, pal.bg_panel, cx))
            .shadow(vec![ring])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(card)
            .child(foot)
            .into_any_element()
    }

    fn tray_card(&self, id: TerminalId, chat: &SurfaceChat, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let now = self.agent_now(id);
        let shell = cx.entity().downgrade();
        let text = div()
            .px(px(SPACE_3))
            .pt(px(SPACE_3))
            .min_h(px(TEXT_MIN_H))
            .child(
                Textarea::new(&chat.input)
                    .appearance(false)
                    .on_paste(move |item, _, cx| {
                        shell
                            .update(cx, |shell, cx| shell.paste_chat_attachment(id, item, cx))
                            .unwrap_or_default()
                    })
                    .disabled(chat.asks().blocks_typing()),
            );
        let structured = chat.structured().is_some();
        let tools = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .px(px(SPACE_1_5))
            .pb(px(SPACE_1_5))
            .pt(px(SPACE_1))
            .children(structured.then(|| Self::attach_control(id, cx)))
            .child(Self::typed_key_tool(
                (MENTION_PREFIX, id.as_u64()),
                IconName::Mention,
                TOOLTIP_MENTION,
                MENTION_KEY,
                id,
                pal,
                cx,
            ))
            .children(structured.then(|| {
                Self::typed_key_tool(
                    (SKILL_PREFIX, id.as_u64()),
                    IconName::CommandLine,
                    TOOLTIP_SKILL,
                    SKILL_KEY,
                    id,
                    pal,
                    cx,
                )
            }))
            .child(div().flex_1())
            .child(Self::send_control(id, chat, now, cx));
        let card = div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .rounded(px(CARD_RADIUS))
            .bg(tinted(pal.shell_ink, CARD_SHADE_ALPHA))
            .border_1()
            .border_color(pal.divider)
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .children(
                Self::attached_tray(id, chat, cx)
                    .map(|tray| div().px(px(SPACE_2)).pt(px(SPACE_2)).child(tray)),
            )
            .children(
                self.kraddie_chip_tray(id, cx)
                    .map(|tray| div().px(px(SPACE_2)).pt(px(SPACE_2)).child(tray)),
            )
            .child(text)
            .child(tools)
            .children(self.suggestion_menu(id, cx));
        Self::suggestion_keys(card, id, cx).into_any_element()
    }

    fn typed_key_tool(
        element: (&'static str, u64),
        glyph: IconName,
        tooltip: &'static str,
        key: &'static str,
        id: TerminalId,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(element)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(TOOL_SIZE))
            .rounded(px(TOOL_RADIUS))
            .cursor_pointer()
            .hover(|style| style.bg(pal.bg_hover))
            .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.type_into_composer(id, key, window, cx);
            }))
            .child(icon(glyph, ICON_SM, pal.text_muted))
            .into_any_element()
    }

    fn type_into_composer(
        &mut self,
        id: TerminalId,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self
            .envs
            .get(&self.active)
            .and_then(|live| live.chats.get(&id))
            .map(|chat| chat.input.clone())
        else {
            return;
        };
        input.update(cx, |field, cx| {
            field.focus(window, cx);
            field.insert(key, window, cx);
        });
    }

    fn tray_foot(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        facts: &Facts,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let structured = chat.structured().is_some();
        let switches = div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SWITCH_GROUP_PAD))
            .p(px(SWITCH_GROUP_PAD))
            .rounded(px(PILL_RADIUS))
            .border_1()
            .border_color(pal.divider)
            .children(Self::tray_thinking(id, facts, cx))
            .children(structured.then(|| self.tray_orchestrator(id, cx)))
            .children(self.chat_mode_control(id, chat, cx));
        let context = context_percent(facts).map(|used| {
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(SPACE_1_5))
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .children(usage_ring(id, facts, pal))
                .child(format!("{used}{PERCENT_SIGN}"))
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_1_5))
            .px(px(SPACE_1))
            .pt(px(SPACE_1_5))
            .pb(px(SPACE_0_5))
            .children(Self::tray_model_pill(id, chat, facts, cx))
            .children(Self::acp_option_controls(id, chat, cx))
            .child(switches)
            .child(div().flex_1())
            .children(context)
            .into_any_element()
    }

    fn tray_model_pill(
        id: TerminalId,
        chat: &SurfaceChat,
        facts: &Facts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let acp = chat
            .structured()
            .is_some_and(|structured| structured.harness.acp_agent().is_some());
        if acp && facts.models.is_empty() {
            return None;
        }
        let pal = palette(cx);
        let (mark, mark_color) = harness_icon(chat.harness(), pal);
        let label = facts
            .model
            .clone()
            .unwrap_or_else(|| MODEL_FALLBACK.to_owned());
        let effort = facts
            .effort
            .or(facts.default_effort)
            .filter(|_| facts.efforts.len() > 1)
            .map(|effort| {
                div()
                    .text_color(rgb(pal.text_muted))
                    .child(format!("{EFFORT_SEPARATOR} {}", effort.label()))
            });
        let groups = std::rc::Rc::new(grouped(
            facts
                .models
                .iter()
                .map(|pick| (pick.harness, ModelChoice::from(pick))),
        ));
        Some(
            div()
                .id((MODEL_PREFIX, id.as_u64()))
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(SPACE_1_5))
                .h(px(PILL_H))
                .pl(px(SPACE_2))
                .pr(px(SPACE_1_5))
                .rounded(px(PILL_RADIUS))
                .border_1()
                .border_color(pal.divider)
                .cursor_pointer()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_default))
                .hover(|style| style.bg(pal.bg_hover))
                .tooltip(|_, cx| tooltip_view(TOOLTIP_MODEL.into(), cx))
                .on_click(
                    cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                        let at = event.position();
                        let origin = (at.x.as_f32(), at.y.as_f32());
                        this.open_model_picker(PickFor::Chat(id), origin, groups.to_vec(), cx);
                    }),
                )
                .when(facts.model_missing, |pill| {
                    pill.child(icon(IconName::Flag, ICON_XS, pal.warning_fg))
                })
                .child(icon(mark, ICON_XS, mark_color))
                .child(label)
                .children(effort)
                .child(icon(IconName::ChevronDown, ICON_XS, pal.text_subtle))
                .into_any_element(),
        )
    }

    fn tray_thinking(id: TerminalId, facts: &Facts, cx: &mut Context<Self>) -> Option<AnyElement> {
        let on = facts.thinking?;
        let tooltip = if on {
            TOOLTIP_THINKING_ON
        } else {
            TOOLTIP_THINKING_OFF
        };
        Some(Self::tray_switch(
            (THINKING_PREFIX, id.as_u64()).into(),
            IconName::Idea,
            on,
            tooltip,
            move |this, _, cx| this.command_surface(id, SurfaceCommand::ToggleThinking, cx),
            cx,
        ))
    }

    fn tray_orchestrator(&self, id: TerminalId, cx: &mut Context<Self>) -> AnyElement {
        let on = self.orchestrates(id);
        let tooltip = if on {
            TOOLTIP_ORCHESTRATOR_ON
        } else {
            TOOLTIP_ORCHESTRATOR_OFF
        };
        Self::tray_switch(
            (ORCHESTRATOR_PREFIX, id.as_u64()).into(),
            IconName::Crown,
            on,
            tooltip,
            move |this, window, cx| this.toggle_orchestrator(id, window, cx),
            cx,
        )
    }

    fn tray_switch(
        element: ElementId,
        glyph: IconName,
        on: bool,
        tooltip: &'static str,
        run: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        div()
            .id(element)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(SWITCH_SIZE))
            .rounded_full()
            .cursor_pointer()
            .when(on, |button| {
                button.bg(row_fill(pal, cx)).shadow(row_shadow(pal, cx))
            })
            .when(!on, |button| button.hover(|style| style.bg(pal.bg_hover)))
            .tooltip(move |_, cx| tooltip_view(SharedString::from(tooltip), cx))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| run(this, window, cx)))
            .child(icon(
                glyph,
                ICON_XS,
                if on { pal.text_strong } else { pal.text_muted },
            ))
            .into_any_element()
    }
}
