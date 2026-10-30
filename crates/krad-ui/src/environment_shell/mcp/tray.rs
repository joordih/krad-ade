use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, InteractiveElement as _, IntoElement,
    ParentElement, StatefulInteractiveElement as _, Styled, Window,
};
use krad_domain::{McpProjectFile, McpProjectTrust, PROJECT_MCP_FILE};
use std::path::Path;
use std::rc::Rc;

use super::super::ask::{tray, tray_button};
use super::super::EnvironmentShell;
use super::server_summary;
use crate::components::{icon, IconName};
use crate::theme::{palette, Palette, ICON, LINE_XS, SPACE_0_5, SPACE_1, SPACE_2, TEXT_XS};

const TRUST_LABEL: &str = "Trust";
const IGNORE_LABEL: &str = "Ignore";
const QUESTION_LEAD: &str = "Start the MCP servers in this project's ";
const QUESTION_TAIL: &str = "?";
const NOTE: &str = "Trusted servers run on this machine with your permissions whenever a chat opens in this folder. Change it later in Settings › MCP.";
const SKIPPED_LEAD: &str = "Skipped ";
const SKIPPED_SEPARATOR: &str = ": ";
const SUMMARY_SEPARATOR: &str = " · ";
const TRUST_ID: &str = "mcp-trust";
const IGNORE_ID: &str = "mcp-ignore";
const LIST_ID: &str = "mcp-trust-servers";
const LIST_MAX_H: f32 = 160.0;
const DIVIDER_H: f32 = 1.0;

type TrustChoice =
    dyn Fn(&mut EnvironmentShell, McpProjectTrust, &mut Window, &mut Context<EnvironmentShell>);

fn server_lines(file: &McpProjectFile, pal: &Palette) -> AnyElement {
    let offered = file.servers.iter().map(|server| {
        div()
            .flex()
            .flex_row()
            .gap(px(SPACE_1))
            .min_w_0()
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(pal.text_strong))
                    .child(server.name.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_muted))
                    .child(format!("{SUMMARY_SEPARATOR}{}", server_summary(server))),
            )
            .into_any_element()
    });
    let skipped = file.skipped.iter().map(|skipped| {
        div()
            .text_color(rgb(pal.text_subtle))
            .child(format!(
                "{SKIPPED_LEAD}{}{SKIPPED_SEPARATOR}{}",
                skipped.name, skipped.reason
            ))
            .into_any_element()
    });
    div()
        .id(LIST_ID)
        .flex()
        .flex_col()
        .gap(px(SPACE_0_5))
        .max_h(px(LIST_MAX_H))
        .overflow_y_scroll()
        .children(offered)
        .children(skipped)
        .child(
            div()
                .pt(px(SPACE_1))
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(NOTE),
        )
        .into_any_element()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn mcp_trust_tray(
        folder: &Path,
        file: &McpProjectFile,
        on_choice: impl Fn(&mut Self, McpProjectTrust, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let on_choice: Rc<TrustChoice> = Rc::new(on_choice);
        let button = |id: &'static str, label: &'static str, trust: McpProjectTrust| {
            let folder = folder.to_path_buf();
            let on_choice = Rc::clone(&on_choice);
            tray_button(
                ElementId::Name(id.into()),
                trust == McpProjectTrust::Trusted,
                pal,
            )
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.set_mcp_project_trust(&folder, trust, cx);
                on_choice(this, trust, window, cx);
            }))
            .child(label)
            .into_any_element()
        };
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(icon(IconName::Plug, ICON, pal.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_default))
                    .child(format!("{QUESTION_LEAD}{PROJECT_MCP_FILE}{QUESTION_TAIL}")),
            )
            .child(button(IGNORE_ID, IGNORE_LABEL, McpProjectTrust::Ignored))
            .child(button(TRUST_ID, TRUST_LABEL, McpProjectTrust::Trusted));
        tray(pal)
            .child(header)
            .child(div().h(px(DIVIDER_H)).bg(pal.divider))
            .child(server_lines(file, pal))
            .into_any_element()
    }
}
