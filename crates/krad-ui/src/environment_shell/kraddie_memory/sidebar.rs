use gpui_kit::{
    div, px, radians, rgb, Animation, AnimationExt as _, AnyElement, Context, ElementId,
    InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, Transformation,
};
use krad_domain::KraddieId;

use super::super::EnvironmentShell;
use crate::components::motion::{ease_out_cubic, rise_in, MOTION_BASE};
use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{
    palette, ICON_XS, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_XS,
};

const EXPANDER_PREFIX: &str = "kraddie-chats-expander";
const EXPANDER_TURN: &str = "kraddie-chats-expander-turn";
const CHAT_ROW_PREFIX: &str = "kraddie-chat-row";
const NEW_CHAT_PREFIX: &str = "kraddie-new-chat";
const EXPAND_TOOLTIP: &str = "Show its chats";
const COLLAPSE_TOOLTIP: &str = "Hide its chats";
const NEW_CHAT_LABEL: &str = "New chat";
const NEW_CHAT_TOOLTIP: &str = "Talk to this Kraddie in a chat of its own";
const ROW_INDENT: f32 = 34.0;
const EXPANDER_SIDE: f32 = 18.0;
const QUARTER_TURN: f32 = std::f32::consts::FRAC_PI_2;
const CHAT_ROW_H: f32 = NAV_ROW_H - SPACE_1;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn kraddie_expander(
        &self,
        index: usize,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let open = self.kraddies.memory.expanded.contains(id);
        let target = id.clone();
        let tooltip = if open {
            COLLAPSE_TOOLTIP
        } else {
            EXPAND_TOOLTIP
        };
        let chevron = icon(IconName::ChevronRight, ICON_XS, pal.text_muted);
        let glyph = if open {
            chevron
                .with_animation(
                    (EXPANDER_TURN, index),
                    Animation::new(MOTION_BASE).with_easing(ease_out_cubic),
                    |glyph, shown| {
                        glyph.with_transformation(Transformation::rotate(radians(
                            QUARTER_TURN * shown,
                        )))
                    },
                )
                .into_any_element()
        } else {
            chevron.into_any_element()
        };
        div()
            .id((EXPANDER_PREFIX, index))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(EXPANDER_SIDE))
            .rounded(px(RADIUS_XS))
            .hover(|style| style.bg(pal.bg_hover))
            .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                cx.stop_propagation();
                if !this.kraddies.memory.expanded.remove(&target) {
                    this.kraddies.memory.expanded.insert(target.clone());
                }
                cx.notify();
            }))
            .child(glyph)
            .into_any_element()
    }

    pub(in crate::environment_shell) fn kraddie_chat_rows(
        &self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !self.kraddies.memory.expanded.contains(id) {
            return Vec::new();
        }
        let pal = palette(cx);
        let chats = self.kraddie_chats(id);
        let mut rows: Vec<AnyElement> = chats
            .into_iter()
            .enumerate()
            .map(|(index, (env, session))| {
                let name = SharedString::from(session.name.clone());
                let session_id = session.id;
                let current = env == self.active
                    && self
                        .envs
                        .get(&env)
                        .is_some_and(|live| live.data.active_session == Some(session_id));
                let row = div()
                    .id(ElementId::Name(
                        format!("{CHAT_ROW_PREFIX}-{id}-{}", session_id.as_u64()).into(),
                    ))
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .h(px(CHAT_ROW_H))
                    .pl(px(ROW_INDENT))
                    .pr(px(SPACE_2))
                    .rounded(px(RADIUS_XS))
                    .cursor_pointer()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(if current {
                        pal.text_strong
                    } else {
                        pal.text_default
                    }))
                    .hover(|style| style.bg(pal.bg_hover))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.reopen_kraddie_chat(&env, session_id, cx);
                    }))
                    .child(icon(IconName::Chat, ICON_XS, pal.text_muted))
                    .child(div().flex_1().min_w_0().truncate().child(name));
                rise_in(
                    ElementId::Name(format!("{CHAT_ROW_PREFIX}-in-{id}-{index}").into()),
                    index,
                    row,
                )
            })
            .collect();
        let target = id.clone();
        let new_chat = div()
            .id(ElementId::Name(format!("{NEW_CHAT_PREFIX}-{id}").into()))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(CHAT_ROW_H))
            .pl(px(ROW_INDENT))
            .pr(px(SPACE_2))
            .mb(px(SPACE_1))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .text_color(rgb(pal.text_muted))
            .hover(|style| style.bg(pal.bg_hover).text_color(rgb(pal.text_strong)))
            .tooltip(|_, cx| tooltip_view(NEW_CHAT_TOOLTIP.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.start_kraddie_chat(&target, cx);
            }))
            .child(icon(IconName::Add, ICON_XS, pal.text_muted))
            .child(NEW_CHAT_LABEL);
        rows.push(rise_in(
            ElementId::Name(format!("{NEW_CHAT_PREFIX}-in-{id}").into()),
            rows.len(),
            new_chat,
        ));
        rows
    }
}
