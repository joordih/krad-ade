use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement, Styled,
    Window,
};
use krad_domain::{AcpAgent, AcpAgentId, McpValue, ToastKind};
use krad_store::{delete_acp_secret, write_acp_secret};

use super::super::super::dialog_parts::{sheet_group, sheet_padded};
use super::super::sections::{card_field, settings_section};
use super::super::{hint, EnvironmentShell};
use super::admit::{admit, Admitted};
use super::draft::{Draft, DraftVariable, StoredSecret, ValueKind};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, SPACE_2};

const ADD_TITLE: &str = "Add an agent";
const EDIT_TITLE: &str = "Edit agent";
const FORM_HINT: &str = "A program that speaks the Agent Client Protocol on its standard input and output. The id names it on the agent bus and cannot change later.";
const ID_LABEL: &str = "Id";
const NAME_LABEL: &str = "Name";
const PROGRAM_LABEL: &str = "Program";
const ARGS_LABEL: &str = "Arguments";
const TERMINAL_LABEL: &str = "Terminal command";
const ID_PLACEHOLDER: &str = "gemini";
const NAME_PLACEHOLDER: &str = "Gemini";
const PROGRAM_PLACEHOLDER: &str = "gemini";
const ARGS_PLACEHOLDER: &str = "--experimental-acp";
const TERMINAL_PLACEHOLDER: &str = "Optional, to open it as a terminal";
const VARIABLE_PLACEHOLDER: &str = "NAME";
const VALUE_PLACEHOLDER: &str = "Value";
const SECRET_PLACEHOLDER: &str = "Secret value";
const KEPT_PLACEHOLDER: &str = "Kept in the keyring; type to replace it";
const VARIABLE_LABEL: &str = "Variable";
const SECRET_SWITCH_LABEL: &str = "Secret";
const ADD_VARIABLE_LABEL: &str = "Add variable";
const ADD_AGENT_LABEL: &str = "Add agent";
const SAVE_AGENT_LABEL: &str = "Save agent";
const CANCEL_LABEL: &str = "Cancel";
const REMOVE_ROW_TOOLTIP: &str = "Remove";
const ADDED_TOAST: &str = "ACP agent saved";
const BTN_CANCEL_FORM: &str = "acp-cancel-form";
const BTN_ADD_VARIABLE: &str = "acp-add-variable";
const BTN_SUBMIT_AGENT: &str = "acp-submit-agent";
const ROW_REMOVE_PREFIX: &str = "acp-row-remove";
const ROW_SECRET_PREFIX: &str = "acp-row-secret";

struct VariableRow {
    id: u64,
    kind: ValueKind,
    stored: StoredSecret,
    name: Entity<InputState>,
    value: Entity<InputState>,
}

pub(super) struct AcpForm {
    id: Entity<InputState>,
    name: Entity<InputState>,
    program: Entity<InputState>,
    args: Entity<InputState>,
    terminal: Entity<InputState>,
    rows: Vec<VariableRow>,
    next_row: u64,
    editing: Option<AcpAgentId>,
    error: Option<String>,
    pub(super) open: bool,
}

fn field(
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<EnvironmentShell>,
) -> Entity<InputState> {
    let placeholder = placeholder.to_owned();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

fn value_field(
    kind: ValueKind,
    stored: StoredSecret,
    text: &str,
    window: &mut Window,
    cx: &mut Context<EnvironmentShell>,
) -> Entity<InputState> {
    let masked = kind == ValueKind::Secret;
    let placeholder = match (kind, stored) {
        (ValueKind::Plain, _) => VALUE_PLACEHOLDER,
        (ValueKind::Secret, StoredSecret::Kept) => KEPT_PLACEHOLDER,
        (ValueKind::Secret, StoredSecret::None) => SECRET_PLACEHOLDER,
    };
    let text = text.to_owned();
    cx.new(|cx| {
        let mut state = InputState::new(window, cx)
            .placeholder(placeholder)
            .masked(masked);
        state.set_value(text, window, cx);
        state
    })
}

impl AcpForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<EnvironmentShell>) -> Self {
        Self {
            id: field(ID_PLACEHOLDER, window, cx),
            name: field(NAME_PLACEHOLDER, window, cx),
            program: field(PROGRAM_PLACEHOLDER, window, cx),
            args: field(ARGS_PLACEHOLDER, window, cx),
            terminal: field(TERMINAL_PLACEHOLDER, window, cx),
            rows: Vec::new(),
            next_row: 0,
            editing: None,
            error: None,
            open: false,
        }
    }

    fn draft(&self, cx: &Context<EnvironmentShell>) -> Draft {
        let text = |field: &Entity<InputState>| field.read(cx).value().to_string();
        Draft {
            id: text(&self.id),
            name: text(&self.name),
            program: text(&self.program),
            args: text(&self.args),
            terminal: text(&self.terminal),
            variables: self
                .rows
                .iter()
                .map(|row| DraftVariable {
                    name: text(&row.name),
                    value: text(&row.value),
                    kind: row.kind,
                    stored: row.stored,
                })
                .collect(),
        }
    }
}

impl EnvironmentShell {
    fn acp_form(&mut self) -> Option<&mut AcpForm> {
        self.settings_ui.as_mut().map(|ui| &mut ui.acp.form)
    }

    pub(super) fn open_acp_form(
        &mut self,
        agent: Option<&AcpAgent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_acp_form(window, cx);
        let rows: Vec<(String, String, ValueKind, StoredSecret)> = agent.map_or_default(|agent| {
            agent
                .env
                .iter()
                .map(|variable| match &variable.value {
                    McpValue::Plain(text) => (
                        variable.name.to_string(),
                        text.clone(),
                        ValueKind::Plain,
                        StoredSecret::None,
                    ),
                    McpValue::Secret => (
                        variable.name.to_string(),
                        String::new(),
                        ValueKind::Secret,
                        StoredSecret::Kept,
                    ),
                })
                .collect()
        });
        for (name, value, kind, stored) in rows {
            self.push_acp_row(&name, &value, kind, stored, window, cx);
        }
        let Some(form) = self.acp_form() else {
            return;
        };
        form.open = true;
        form.editing = agent.map(|agent| agent.id);
        let filled = agent.map(|agent| {
            [
                (form.id.clone(), agent.id.to_string()),
                (form.name.clone(), agent.name.clone()),
                (form.program.clone(), agent.program.clone()),
                (form.args.clone(), agent.args.join(" ")),
                (
                    form.terminal.clone(),
                    agent.terminal.clone().unwrap_or_default(),
                ),
            ]
        });
        for (field, text) in filled.into_iter().flatten() {
            field.update(cx, |field, cx| field.set_value(text, window, cx));
        }
        cx.notify();
    }

    fn push_acp_row(
        &mut self,
        name: &str,
        value: &str,
        kind: ValueKind,
        stored: StoredSecret,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name_field = field(VARIABLE_PLACEHOLDER, window, cx);
        let text = name.to_owned();
        name_field.update(cx, |field, cx| field.set_value(text, window, cx));
        let value = value_field(kind, stored, value, window, cx);
        if let Some(form) = self.acp_form() {
            form.next_row += 1;
            form.rows.push(VariableRow {
                id: form.next_row,
                kind,
                stored,
                name: name_field,
                value,
            });
        }
        cx.notify();
    }

    fn flip_acp_secret(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self
            .acp_form()
            .and_then(|form| form.rows.iter().find(|row| row.id == id))
        else {
            return;
        };
        let kind = match row.kind {
            ValueKind::Plain => ValueKind::Secret,
            ValueKind::Secret => ValueKind::Plain,
        };
        let stored = row.stored;
        let text = row.value.read(cx).value().to_string();
        let value = value_field(kind, stored, &text, window, cx);
        if let Some(row) = self
            .acp_form()
            .and_then(|form| form.rows.iter_mut().find(|row| row.id == id))
        {
            row.kind = kind;
            row.value = value;
        }
        cx.notify();
    }

    fn remove_acp_row(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(form) = self.acp_form() {
            form.rows.retain(|row| row.id != id);
        }
        cx.notify();
    }

    fn close_acp_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_acp_form(window, cx);
        cx.notify();
    }

    fn clear_acp_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.acp_form() else {
            return;
        };
        form.rows.clear();
        form.error = None;
        form.editing = None;
        form.open = false;
        let fields = [
            form.id.clone(),
            form.name.clone(),
            form.program.clone(),
            form.args.clone(),
            form.terminal.clone(),
        ];
        for field in fields {
            field.update(cx, |field, cx| field.set_value("", window, cx));
        }
    }

    fn submit_acp_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((draft, editing)) = self
            .settings_ui
            .as_ref()
            .map(|ui| (ui.acp.form.draft(cx), ui.acp.form.editing))
        else {
            return;
        };
        match admit(&self.settings.acp, editing, &draft) {
            Ok(admitted) => {
                self.clear_acp_form(window, cx);
                Self::save_acp_agent(admitted, cx);
            }
            Err(error) => {
                if let Some(form) = self.acp_form() {
                    form.error = Some(error);
                }
            }
        }
        cx.notify();
    }

    fn save_acp_agent(admitted: Admitted, cx: &mut Context<Self>) {
        let Admitted {
            acp,
            writes,
            deletes,
        } = admitted;
        let stored = cx.background_spawn(async move {
            writes
                .iter()
                .try_for_each(|(key, secret)| write_acp_secret(key, secret))?;
            deletes.iter().try_for_each(delete_acp_secret)
        });
        cx.spawn(async move |this, cx| {
            let stored = stored.await;
            this.update(cx, |shell, cx| {
                if let Err(error) = stored {
                    return shell.toast(ToastKind::Error, &error.to_string(), cx);
                }
                shell.settings.acp = acp;
                shell.save_settings(cx);
                shell.detect_agents(cx);
                shell.toast(ToastKind::Success, ADDED_TOAST, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn acp_form_section(form: &AcpForm, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let editing = form.editing.is_some();
        let mut fields = vec![
            card_field(
                ID_LABEL,
                Input::new(&form.id).disabled(editing).into_any_element(),
                pal,
            ),
            card_field(NAME_LABEL, Input::new(&form.name).into_any_element(), pal),
            card_field(
                PROGRAM_LABEL,
                Input::new(&form.program).into_any_element(),
                pal,
            ),
            card_field(ARGS_LABEL, Input::new(&form.args).into_any_element(), pal),
            card_field(
                TERMINAL_LABEL,
                Input::new(&form.terminal).into_any_element(),
                pal,
            ),
        ];
        fields.extend(form.rows.iter().map(|row| Self::acp_row(row, cx)));
        fields.extend(
            form.error
                .as_deref()
                .map(|error| sheet_padded(hint(error, pal)).into_any_element()),
        );
        fields.push(sheet_padded(Self::acp_form_buttons(editing, cx)).into_any_element());
        let title = if editing { EDIT_TITLE } else { ADD_TITLE };
        settings_section(title, Some(FORM_HINT), [sheet_group(fields, pal)], pal)
    }

    fn acp_row(row: &VariableRow, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let id = row.id;
        let value = match row.kind {
            ValueKind::Secret => Input::new(&row.value).mask_toggle(),
            ValueKind::Plain => Input::new(&row.value),
        };
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(div().flex_1().min_w_0().child(Input::new(&row.name)))
            .child(div().flex_1().min_w_0().child(value))
            .child(
                Switch::new((ROW_SECRET_PREFIX, id))
                    .label(SECRET_SWITCH_LABEL)
                    .checked(row.kind == ValueKind::Secret)
                    .on_click(cx.listener(move |this: &mut Self, _: &bool, window, cx| {
                        this.flip_acp_secret(id, window, cx);
                    })),
            )
            .child(
                Button::new((ROW_REMOVE_PREFIX, id))
                    .icon(IconName::Close)
                    .small()
                    .tooltip(REMOVE_ROW_TOOLTIP)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.remove_acp_row(id, cx);
                    })),
            )
            .into_any_element();
        card_field(VARIABLE_LABEL, control, pal)
    }

    fn acp_form_buttons(editing: bool, cx: &mut Context<Self>) -> AnyElement {
        let submit = if editing {
            SAVE_AGENT_LABEL
        } else {
            ADD_AGENT_LABEL
        };
        div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .child(
                Button::new(BTN_ADD_VARIABLE)
                    .label(ADD_VARIABLE_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.push_acp_row("", "", ValueKind::Plain, StoredSecret::None, window, cx);
                    })),
            )
            .child(div().flex_1())
            .child(
                Button::new(BTN_CANCEL_FORM)
                    .label(CANCEL_LABEL)
                    .small()
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.close_acp_form(window, cx);
                    })),
            )
            .child(
                Button::new(BTN_SUBMIT_AGENT)
                    .label(submit)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.submit_acp_agent(window, cx);
                    })),
            )
            .into_any_element()
    }
}
