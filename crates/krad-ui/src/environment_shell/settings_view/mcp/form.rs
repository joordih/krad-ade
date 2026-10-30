use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, Window,
};
use krad_domain::{McpSecretKey, McpServerTaken};
use krad_store::write_mcp_secret;

use super::super::super::dialog_parts::{segment, SegmentSize};
use super::super::gitlab::page::{field_row, section_heading};
use super::super::{hint, segmented_track, EnvironmentShell};
use super::draft::{Draft, DraftVariable, McpKind, ReadyServer, ValueKind};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, SPACE_2};

const FORM_TITLE: &str = "Add a server";
const FORM_HINT: &str = "A command the app starts, or the address of a streamable HTTP server. Secret values and headers are kept in the system keyring.";
const KIND_LABEL: &str = "Transport";
const STDIO_LABEL: &str = "Command";
const HTTP_LABEL: &str = "HTTP";
const NAME_LABEL: &str = "Name";
const COMMAND_LABEL: &str = "Command";
const ARGS_LABEL: &str = "Arguments";
const URL_LABEL: &str = "Address";
const NAME_PLACEHOLDER: &str = "github";
const COMMAND_PLACEHOLDER: &str = "npx";
const ARGS_PLACEHOLDER: &str = "-y @modelcontextprotocol/server-github";
const URL_PLACEHOLDER: &str = "https://mcp.example.com/mcp";
const VARIABLE_PLACEHOLDER: &str = "NAME";
const HEADER_PLACEHOLDER: &str = "Authorization";
const VALUE_PLACEHOLDER: &str = "Value";
const SECRET_PLACEHOLDER: &str = "Secret value";
const ADD_VARIABLE_LABEL: &str = "Add variable";
const ADD_SECRET_LABEL: &str = "Add secret";
const ADD_HEADER_LABEL: &str = "Add header";
const ADD_SERVER_LABEL: &str = "Add server";
const REMOVE_ROW_TOOLTIP: &str = "Remove";
const VARIABLE_LABEL: &str = "Variable";
const SECRET_LABEL: &str = "Secret";
const HEADER_LABEL: &str = "Header";
const ADDED_TOAST: &str = "MCP server added";
const BTN_STDIO: &str = "mcp-kind-stdio";
const BTN_HTTP: &str = "mcp-kind-http";
const BTN_ADD_VARIABLE: &str = "mcp-add-variable";
const BTN_ADD_SECRET: &str = "mcp-add-secret";
const BTN_ADD_SERVER: &str = "mcp-add-server";
const ROW_REMOVE_PREFIX: &str = "mcp-row-remove";
const FIELD_GAP: f32 = 10.0;

struct VariableRow {
    id: u64,
    kind: ValueKind,
    name: Entity<InputState>,
    value: Entity<InputState>,
}

pub(in crate::environment_shell) struct McpForm {
    kind: McpKind,
    name: Entity<InputState>,
    command: Entity<InputState>,
    args: Entity<InputState>,
    url: Entity<InputState>,
    rows: Vec<VariableRow>,
    next_row: u64,
    error: Option<String>,
}

fn field(
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<EnvironmentShell>,
) -> Entity<InputState> {
    let placeholder = placeholder.to_owned();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

impl McpForm {
    pub(in crate::environment_shell) fn new(
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) -> Self {
        Self {
            kind: McpKind::Stdio,
            name: field(NAME_PLACEHOLDER, window, cx),
            command: field(COMMAND_PLACEHOLDER, window, cx),
            args: field(ARGS_PLACEHOLDER, window, cx),
            url: field(URL_PLACEHOLDER, window, cx),
            rows: Vec::new(),
            next_row: 0,
            error: None,
        }
    }

    fn draft(&self, cx: &Context<EnvironmentShell>) -> Draft {
        let text = |field: &Entity<InputState>| field.read(cx).value().to_string();
        Draft {
            kind: self.kind,
            name: text(&self.name),
            command: text(&self.command),
            args: text(&self.args),
            url: text(&self.url),
            variables: self
                .rows
                .iter()
                .map(|row| DraftVariable {
                    name: text(&row.name),
                    value: text(&row.value),
                    kind: row.kind,
                })
                .collect(),
        }
    }
}

const fn row_label(form: McpKind, kind: ValueKind) -> &'static str {
    match (form, kind) {
        (McpKind::Http, _) => HEADER_LABEL,
        (McpKind::Stdio, ValueKind::Plain) => VARIABLE_LABEL,
        (McpKind::Stdio, ValueKind::Secret) => SECRET_LABEL,
    }
}

impl EnvironmentShell {
    fn mcp_form(&mut self) -> Option<&mut McpForm> {
        self.settings_ui.as_mut().map(|ui| &mut ui.mcp.form)
    }

    fn set_mcp_kind(&mut self, kind: McpKind, cx: &mut Context<Self>) {
        if let Some(form) = self.mcp_form() {
            if form.kind != kind {
                form.kind = kind;
                form.rows.clear();
                form.error = None;
            }
        }
        cx.notify();
    }

    fn add_mcp_row(&mut self, kind: ValueKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form_kind) = self.mcp_form().map(|form| form.kind) else {
            return;
        };
        let masked = form_kind == McpKind::Http || kind == ValueKind::Secret;
        let (name_hint, value_hint) = match (form_kind, masked) {
            (McpKind::Http, _) => (HEADER_PLACEHOLDER, SECRET_PLACEHOLDER),
            (McpKind::Stdio, true) => (VARIABLE_PLACEHOLDER, SECRET_PLACEHOLDER),
            (McpKind::Stdio, false) => (VARIABLE_PLACEHOLDER, VALUE_PLACEHOLDER),
        };
        let name = field(name_hint, window, cx);
        let value = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(value_hint)
                .masked(masked)
        });
        if let Some(form) = self.mcp_form() {
            form.next_row += 1;
            form.rows.push(VariableRow {
                id: form.next_row,
                kind,
                name,
                value,
            });
        }
        cx.notify();
    }

    fn remove_mcp_row(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(form) = self.mcp_form() {
            form.rows.retain(|row| row.id != id);
        }
        cx.notify();
    }

    fn submit_mcp_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.settings_ui.as_ref().map(|ui| ui.mcp.form.draft(cx)) else {
            return;
        };
        let ready = draft
            .ready()
            .map_err(|error| error.to_string())
            .and_then(|ready| {
                if self
                    .settings
                    .mcp
                    .servers
                    .iter()
                    .any(|kept| kept.name == ready.server.name)
                {
                    Err(McpServerTaken(ready.server.name).to_string())
                } else {
                    Ok(ready)
                }
            });
        match ready {
            Ok(ready) => {
                self.clear_mcp_form(window, cx);
                Self::save_mcp_server(ready, cx);
            }
            Err(error) => {
                if let Some(form) = self.mcp_form() {
                    form.error = Some(error);
                }
                cx.notify();
            }
        }
    }

    fn clear_mcp_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.mcp_form() else {
            return;
        };
        form.rows.clear();
        form.error = None;
        let fields = [
            form.name.clone(),
            form.command.clone(),
            form.args.clone(),
            form.url.clone(),
        ];
        for field in fields {
            field.update(cx, |field, cx| field.set_value("", window, cx));
        }
    }

    fn save_mcp_server(ready: ReadyServer, cx: &mut Context<Self>) {
        let ReadyServer { server, secrets } = ready;
        let name = server.name.clone();
        let written = cx.background_spawn(async move {
            secrets.iter().try_for_each(|(variable, secret)| {
                let key = McpSecretKey {
                    server: name.clone(),
                    variable: variable.clone(),
                };
                write_mcp_secret(&key, secret)
            })
        });
        cx.spawn(async move |this, cx| {
            let written = written.await;
            this.update(cx, |shell, cx| {
                if let Err(error) = written {
                    return shell.toast(&error.to_string(), cx);
                }
                match shell.settings.mcp.add_server(server) {
                    Ok(()) => {
                        shell.save_settings(cx);
                        shell.toast(ADDED_TOAST, cx);
                    }
                    Err(taken) => shell.toast(&taken.to_string(), cx),
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn mcp_form_section(form: &McpForm, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let kind = form.kind;
        let option = |id: &'static str, label: &'static str, choice: McpKind| {
            segment(
                id,
                label,
                None,
                kind == choice,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| this.set_mcp_kind(choice, cx)))
        };
        let kinds = segmented_track(pal)
            .child(option(BTN_STDIO, STDIO_LABEL, McpKind::Stdio))
            .child(option(BTN_HTTP, HTTP_LABEL, McpKind::Http))
            .into_any_element();
        let mut fields = vec![
            field_row(KIND_LABEL, kinds, pal),
            field_row(NAME_LABEL, Input::new(&form.name).into_any_element(), pal),
        ];
        match kind {
            McpKind::Stdio => {
                fields.push(field_row(
                    COMMAND_LABEL,
                    Input::new(&form.command).into_any_element(),
                    pal,
                ));
                fields.push(field_row(
                    ARGS_LABEL,
                    Input::new(&form.args).into_any_element(),
                    pal,
                ));
            }
            McpKind::Http => {
                fields.push(field_row(
                    URL_LABEL,
                    Input::new(&form.url).into_any_element(),
                    pal,
                ));
            }
        }
        fields.extend(form.rows.iter().map(|row| Self::mcp_row(kind, row, cx)));
        div()
            .flex()
            .flex_col()
            .gap(px(FIELD_GAP))
            .child(section_heading(FORM_TITLE, Some(FORM_HINT), pal))
            .children(fields)
            .children(form.error.as_deref().map(|error| hint(error, pal)))
            .child(Self::mcp_form_buttons(kind, cx))
            .into_any_element()
    }

    fn mcp_row(form: McpKind, row: &VariableRow, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let id = row.id;
        let value = if form == McpKind::Http || row.kind == ValueKind::Secret {
            Input::new(&row.value).mask_toggle()
        } else {
            Input::new(&row.value)
        };
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(div().flex_1().min_w_0().child(Input::new(&row.name)))
            .child(div().flex_1().min_w_0().child(value))
            .child(
                Button::new((ROW_REMOVE_PREFIX, id))
                    .icon(IconName::Close)
                    .small()
                    .tooltip(REMOVE_ROW_TOOLTIP)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.remove_mcp_row(id, cx);
                    })),
            )
            .into_any_element();
        field_row(row_label(form, row.kind), control, pal)
    }

    fn mcp_form_buttons(kind: McpKind, cx: &mut Context<Self>) -> AnyElement {
        let add = |id: &'static str, label: &'static str, value: ValueKind| {
            Button::new(id)
                .label(label)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.add_mcp_row(value, window, cx);
                }))
        };
        let mut buttons = div().flex().flex_row().gap(px(SPACE_2));
        buttons = match kind {
            McpKind::Stdio => buttons
                .child(add(BTN_ADD_VARIABLE, ADD_VARIABLE_LABEL, ValueKind::Plain))
                .child(add(BTN_ADD_SECRET, ADD_SECRET_LABEL, ValueKind::Secret)),
            McpKind::Http => {
                buttons.child(add(BTN_ADD_VARIABLE, ADD_HEADER_LABEL, ValueKind::Secret))
            }
        };
        buttons
            .child(div().flex_1())
            .child(
                Button::new(BTN_ADD_SERVER)
                    .label(ADD_SERVER_LABEL)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.submit_mcp_server(window, cx);
                    })),
            )
            .into_any_element()
    }
}
