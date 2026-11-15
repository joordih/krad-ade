use gpui_kit::{div, px, rgb, AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled};

use super::super::super::EnvironmentShell;
use super::HostedChatKey;
use crate::components::{card, Button, ButtonVariant};
use crate::theme::{palette, SPACE_2, SPACE_3, TEXT_XS};

const HEADER_H: f32 = 36.0;
const CLOSE_ID: &str = "hosted-chat-close";
const CLOSE_LABEL: &str = "Close";
const GONE: &str = "This cloud chat is no longer on the runner";
const ON_RUNNER: &str = "on";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn hosted_chat_pane(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let key = self.cloud.shown.clone()?;
        let pal = palette(cx);
        let place = key.place();
        let id = key.chat.terminal;
        let header = self.hosted_header(&key, cx);
        let body = self
            .live(&place)
            .and_then(|live| live.chats.get(&id))
            .map_or_else(
                || {
                    div()
                        .p(px(SPACE_3))
                        .text_color(rgb(pal.text_muted))
                        .child(GONE)
                        .into_any_element()
                },
                |chat| self.surface_chat_body(id, chat, cx),
            );
        Some(
            card(pal, cx)
                .size_full()
                .flex()
                .flex_col()
                .child(header)
                .child(div().flex().flex_col().flex_1().min_h_0().child(body))
                .into_any_element(),
        )
    }

    fn hosted_header(&self, key: &HostedChatKey, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let harness = self
            .runtime
            .view
            .read(cx)
            .remotes
            .chat(key.runner, &key.chat)
            .map(|chat| chat.mirror.state.harness);
        let title =
            harness.map(|harness| self.runtime.settings.acp.harness_name(harness).into_owned());
        let caption = format!("{ON_RUNNER} {}", self.runner_name(key.runner));
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(HEADER_H))
            .px(px(SPACE_3))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .children(title),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(caption),
            )
            .child(
                Button::new(CLOSE_ID)
                    .label(CLOSE_LABEL)
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.leave_hosted_chat();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
