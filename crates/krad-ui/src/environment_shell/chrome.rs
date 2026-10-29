use std::time::{Duration, Instant};

use gpui_kit::{
    div, px, rgb, AnyElement, App, ClickEvent, Context, IntoElement, ParentElement, Styled, Window,
};

use super::EnvironmentShell;
use crate::components::{Button, IconName};
use crate::theme::{
    palette, popover_fill, popover_shadow, LINE_SM, RADIUS_MD, SPACE_1, SPACE_2, SPACE_3, TEXT_SM,
    TOOLBAR_H, WINDOW_INSET,
};

const BTN_CLOSE: &str = "close";

const OVERLAY_GAP: f32 = SPACE_2;
const TOAST_TOP: f32 = WINDOW_INSET + TOOLBAR_H + WINDOW_INSET + OVERLAY_GAP;
const TOAST_RIGHT: f32 = WINDOW_INSET + SPACE_3;
const TOAST_WIDTH: f32 = 320.0;
const TOAST_CAP: usize = 5;
const TOAST_TTL: Duration = Duration::from_secs(6);

pub(super) struct Toast {
    seq: u64,
    text: String,
    born: Instant,
}

impl EnvironmentShell {
    pub(super) fn toast_stack(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        if self.toasts.is_empty() {
            return div().into_any_element();
        }
        let items: Vec<AnyElement> = self
            .toasts
            .iter()
            .map(|toast| {
                let seq = toast.seq;
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(SPACE_2))
                    .pl(px(SPACE_3))
                    .pr(px(SPACE_1))
                    .py(px(SPACE_1))
                    .bg(popover_fill(pal, cx))
                    .border_1()
                    .border_color(pal.border)
                    .rounded(px(RADIUS_MD))
                    .shadow(popover_shadow(pal, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .text_color(rgb(pal.text_default))
                            .child(toast.text.clone()),
                    )
                    .child(Self::close_button(
                        format!("{BTN_CLOSE}-toast-{seq}"),
                        cx.listener(move |this: &mut Self, _, _, cx| {
                            this.toasts.retain(|toast| toast.seq != seq);
                            cx.notify();
                        }),
                    ))
                    .into_any_element()
            })
            .collect();
        div()
            .absolute()
            .top(px(TOAST_TOP))
            .right(px(TOAST_RIGHT))
            .w(px(TOAST_WIDTH))
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .children(items)
            .into_any_element()
    }

    fn close_button(
        id: String,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Button {
        Button::new(id)
            .icon(IconName::Close)
            .small()
            .on_click(on_click)
    }

    pub(super) fn push_toast(&mut self, text: String) {
        self.activity_log.record(text.clone());
        self.toast_seq += 1;
        self.toasts.push(Toast {
            seq: self.toast_seq,
            text,
            born: Instant::now(),
        });
        while self.toasts.len() > TOAST_CAP {
            self.toasts.remove(0);
        }
    }

    pub(super) fn expire_toasts(&mut self, now: Instant) -> bool {
        let before = self.toasts.len();
        self.toasts
            .retain(|toast| now.duration_since(toast.born) < TOAST_TTL);
        self.toasts.len() != before
    }
}
