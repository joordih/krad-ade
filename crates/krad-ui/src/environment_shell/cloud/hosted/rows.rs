use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{AgentState, CloudRunnerId, GroupId};
use krad_protocol::{AgentRef, HostedSummary};

use super::super::super::terminal_status::CardStatus;
use super::super::super::EnvironmentShell;
use super::super::cloud_marker;
use super::{hosted_in, HostedChatKey};
use crate::components::{agent_status, icon, IconName};
use crate::theme::{
    palette, row_fill, row_shadow, ICON_SM, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_1,
    SPACE_1_5, SPACE_2, TEXT_SM, TEXT_XS,
};

const HOSTED_ROW_PREFIX: &str = "hosted-session-row";
const CAPTION_SEPARATOR: &str = " · ";
const RUNNER_SHIFT: u32 = 32;

struct HostedRow {
    key: Option<HostedChatKey>,
    name: String,
    caption: String,
    state: AgentState,
    marker: u64,
    active: bool,
}

fn row_marker(runner: CloudRunnerId, summary: &HostedSummary) -> u64 {
    (runner.0 << RUNNER_SHIFT) | summary.session.as_u64()
}

fn first_chat(runner: CloudRunnerId, summary: &HostedSummary) -> Option<HostedChatKey> {
    let chat = summary.chats.first()?;
    Some(HostedChatKey {
        runner,
        chat: AgentRef {
            environment: summary.environment.clone(),
            terminal: chat.target.terminal,
        },
    })
}

fn caption(summary: &HostedSummary) -> String {
    let chat = summary.chats.first();
    let model = chat.and_then(|chat| chat.model.as_deref());
    let effort = chat
        .and_then(|chat| chat.effort)
        .map(|effort| effort.to_string());
    model
        .into_iter()
        .map(str::to_owned)
        .chain(effort)
        .chain([summary.branch.clone()])
        .collect::<Vec<_>>()
        .join(CAPTION_SEPARATOR)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn hosted_rows(
        &self,
        group: Option<GroupId>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let view = self.runtime.view.read(cx);
        let rows: Vec<(CloudRunnerId, HostedRow)> =
            hosted_in(view.remotes.hosted(), &self.active, group)
                .into_iter()
                .map(|(runner, summary)| {
                    let key = first_chat(runner, summary);
                    let row = HostedRow {
                        active: key.is_some() && self.cloud.shown == key,
                        key,
                        name: summary.name.clone(),
                        caption: caption(summary),
                        state: summary.state(),
                        marker: row_marker(runner, summary),
                    };
                    (runner, row)
                })
                .collect();
        rows.into_iter()
            .map(|(runner, row)| {
                let waiting = row.state == AgentState::WaitingForInput;
                let badge = self.cloud_badge(runner, waiting, cx);
                let marker = cloud_marker(row.marker, badge, cx);
                Self::hosted_row(row, marker, cx)
            })
            .collect()
    }

    fn hosted_row(row: HostedRow, marker: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let quiet = matches!(row.state, AgentState::Idle | AgentState::Done);
        let key = row.key.clone();
        div()
            .id((HOSTED_ROW_PREFIX, row.marker))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(NAV_ROW_H))
            .pl(px(SPACE_1))
            .pr(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(pal.text_strong))
            .when(row.active, |element| {
                element.bg(row_fill(pal, cx)).shadow(row_shadow(pal, cx))
            })
            .hover(|style| style.bg(pal.bg_hover))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                if let Some(key) = key.clone() {
                    this.leave_main_overlays();
                    this.show_hosted_chat(key, window, cx);
                }
            }))
            .child(icon(IconName::Layers, ICON_SM, pal.text_muted))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(row.name))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(rgb(pal.text_muted))
                            .child(row.caption),
                    ),
            )
            .child(marker)
            .when(!quiet, |element| {
                element.child(agent_status(
                    CardStatus::Agent(row.state).kind(),
                    ICON_SM,
                    pal,
                ))
            })
            .into_any_element()
    }
}
