mod draft;
mod form;

use gpui_kit::component::switch::Switch;
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, IntoElement, ParentElement, Styled, Window,
};
use krad_domain::{McpProjectTrust, McpServer, McpServerName, McpServerState};
use krad_store::delete_mcp_secret;
use std::path::{Path, PathBuf};

use super::super::mcp::server_summary;
use super::sections::{empty_row, footnote, pane_column, settings_section};
use super::{form_row, panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, SPACE_2};
use form::McpForm;

const SECRETS_NOTE: &str = "Secret values and headers are kept in the system keyring. Without one, as under WSL, they go in the config folder, readable only by you.";
const ADD_SERVER_LABEL: &str = "Add server";
const BTN_OPEN_FORM: &str = "mcp-open-form";
const SERVERS_TITLE: &str = "Servers";
const SERVERS_HINT: &str = "Servers that are on start with every Gateway chat. Each tool call goes through the chat's permission mode, like a shell command.";
const SERVERS_EMPTY: &str = "No servers yet.";
const PROJECTS_TITLE: &str = "Project servers";
const PROJECTS_HINT: &str =
    "Folders whose .mcp.json you trusted or ignored. Forget a choice to be asked again.";
const TRUSTED_DETAIL: &str = "Trusted";
const IGNORED_DETAIL: &str = "Ignored";
const REMOVE_LABEL: &str = "Remove";
const FORGET_LABEL: &str = "Forget";
const REMOVED_TOAST: &str = "MCP server removed";
const SERVER_SWITCH_PREFIX: &str = "mcp-server-switch";
const SERVER_REMOVE_PREFIX: &str = "mcp-server-remove";
const PROJECT_SWITCH_PREFIX: &str = "mcp-project-switch";
const PROJECT_FORGET_PREFIX: &str = "mcp-project-forget";

pub(in crate::environment_shell) struct McpUi {
    form: McpForm,
}

fn element_index(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

const fn trust_detail(trust: McpProjectTrust) -> &'static str {
    match trust {
        McpProjectTrust::Trusted => TRUSTED_DETAIL,
        McpProjectTrust::Ignored => IGNORED_DETAIL,
    }
}

const fn flipped(trust: McpProjectTrust) -> McpProjectTrust {
    match trust {
        McpProjectTrust::Trusted => McpProjectTrust::Ignored,
        McpProjectTrust::Ignored => McpProjectTrust::Trusted,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn mcp_ui(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> McpUi {
        McpUi {
            form: McpForm::new(window, cx),
        }
    }

    fn toggle_mcp_server(&mut self, name: &McpServerName, cx: &mut Context<Self>) {
        self.settings.mcp.toggle_server(name);
        self.save_settings(cx);
        cx.notify();
    }

    fn remove_mcp_server(&mut self, name: &McpServerName, cx: &mut Context<Self>) {
        let Some(server) = self.settings.mcp.remove_server(name) else {
            return;
        };
        self.save_settings(cx);
        cx.notify();
        let removed = cx.background_spawn(async move {
            server.secret_keys().iter().try_for_each(delete_mcp_secret)
        });
        cx.spawn(async move |this, cx| {
            let removed = removed.await;
            this.update(cx, |shell, cx| match removed {
                Ok(()) => shell.toast(REMOVED_TOAST, cx),
                Err(error) => shell.toast(&error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn mcp_server_row(
        index: usize,
        server: &McpServer,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let toggled = server.name.clone();
        let removed = server.name.clone();
        let controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                Switch::new((SERVER_SWITCH_PREFIX, element_index(index)))
                    .checked(server.state == McpServerState::Enabled)
                    .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
                        this.toggle_mcp_server(&toggled, cx);
                    })),
            )
            .child(
                Button::new((SERVER_REMOVE_PREFIX, element_index(index)))
                    .label(REMOVE_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.remove_mcp_server(&removed, cx);
                    })),
            )
            .into_any_element();
        form_row(
            server.name.as_str(),
            Some(&server_summary(server)),
            controls,
            pal,
        )
    }

    fn mcp_servers_card(&self, form_open: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rows: Vec<AnyElement> = if self.settings.mcp.servers.is_empty() {
            vec![empty_row(SERVERS_EMPTY, pal)]
        } else {
            self.settings
                .mcp
                .servers
                .iter()
                .enumerate()
                .map(|(index, server)| Self::mcp_server_row(index, server, pal, cx))
                .collect()
        };
        let add = (!form_open).then(|| {
            div()
                .flex()
                .flex_row()
                .justify_end()
                .child(
                    Button::new(BTN_OPEN_FORM)
                        .label(ADD_SERVER_LABEL)
                        .icon(IconName::Add)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.show_mcp_form(true, cx);
                        })),
                )
                .into_any_element()
        });
        settings_section(
            SERVERS_TITLE,
            Some(SERVERS_HINT),
            std::iter::once(panel_card(rows, pal)).chain(add),
            pal,
        )
    }

    fn mcp_project_row(
        index: usize,
        folder: &Path,
        trust: McpProjectTrust,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let switched: PathBuf = folder.to_path_buf();
        let forgotten: PathBuf = folder.to_path_buf();
        let controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                Switch::new((PROJECT_SWITCH_PREFIX, element_index(index)))
                    .checked(trust == McpProjectTrust::Trusted)
                    .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
                        this.set_mcp_project_trust(&switched, flipped(trust), cx);
                    })),
            )
            .child(
                Button::new((PROJECT_FORGET_PREFIX, element_index(index)))
                    .label(FORGET_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.forget_mcp_project(&forgotten, cx);
                    })),
            )
            .into_any_element();
        form_row(
            &folder.display().to_string(),
            Some(trust_detail(trust)),
            controls,
            pal,
        )
    }

    fn mcp_projects_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rows: Vec<AnyElement> = self
            .settings
            .mcp
            .projects
            .iter()
            .enumerate()
            .map(|(index, (folder, trust))| Self::mcp_project_row(index, folder, *trust, pal, cx))
            .collect();
        settings_section(
            PROJECTS_TITLE,
            Some(PROJECTS_HINT),
            [panel_card(rows, pal)],
            pal,
        )
    }

    pub(super) fn mcp_pane(&self, ui: &McpUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let open = ui.form.open;
        let mut sections = vec![self.mcp_servers_card(open, cx)];
        if open {
            sections.push(Self::mcp_form_section(&ui.form, cx));
        }
        if !self.settings.mcp.projects.is_empty() {
            sections.push(self.mcp_projects_card(cx));
        }
        sections.push(footnote(SECRETS_NOTE, pal));
        pane_column(sections)
    }
}
