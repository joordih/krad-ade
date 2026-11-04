mod models;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement, Styled,
    Subscription, Window,
};
use krad_domain::{GatewayKey, GatewayUrl, HttpUrlError, EXAMPLE_GATEWAY_URL};
use krad_store::LoginPlace;

use super::sections::{
    account_row, card_field, footnote, pane_column, settings_section, AccountRow,
};
use super::{hint, panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant, IconName};
use crate::environment_shell::gateway::GatewayLink;
use crate::theme::{palette, SPACE_1, SPACE_2};

const KEY_PLACEHOLDER: &str = "An API key the gateway accepts";
const GATEWAY_NAME: &str = "Gateway";
const CONNECTION_HINT: &str = "A gateway you run, such as CLIProxyAPI, that serves the models of your accounts. Start it with its own tools, then add its address and a key here.";
const KEY_NOTE: &str = "The key is kept in the system keyring. Without one, as under WSL, it goes in the config folder, readable only by you.";
const CONNECTION_TITLE: &str = "Connection";
const URL_LABEL: &str = "Address";
const KEY_LABEL: &str = "Key";
const SAVE_KEY_LABEL: &str = "Save key";
const CHECK_LABEL: &str = "Check again";
const FORGET_LABEL: &str = "Forget key";
const CHECKING_TITLE: &str = "Checking the Gateway…";
const UNSET_TITLE: &str = "Not connected";
const UNSET_DETAIL: &str = "Add the address and a key to connect.";
const READY_TITLE: &str = "Connected";
const FAILED_TITLE: &str = "The Gateway did not answer";
const KEYRING_PLACE: &str = "Key in the system keyring";
const CONFIG_FOLDER_PLACE: &str = "Key in the config folder";
const DETAIL_SEPARATOR: &str = " · ";
const MODELS_SUFFIX: &str = "models";
const BTN_SAVE_KEY: &str = "gateway-save-key";
const BTN_CHECK: &str = "gateway-check";
const BTN_FORGET: &str = "gateway-forget";

pub(in crate::environment_shell) struct GatewayUi {
    url: Entity<InputState>,
    key: Entity<InputState>,
    url_error: Option<HttpUrlError>,
    _edits: Subscription,
}

const fn place_text(place: LoginPlace) -> &'static str {
    match place {
        LoginPlace::Keyring => KEYRING_PLACE,
        LoginPlace::ConfigFolder => CONFIG_FOLDER_PLACE,
    }
}

fn read_url(text: &str) -> Result<Option<GatewayUrl>, HttpUrlError> {
    match text.parse() {
        Ok(url) => Ok(Some(url)),
        Err(HttpUrlError::Empty) => Ok(None),
        Err(error) => Err(error),
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn gateway_ui(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> GatewayUi {
        let saved = self
            .settings
            .gateway
            .url
            .as_ref()
            .map_or_else(String::new, ToString::to_string);
        let url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(EXAMPLE_GATEWAY_URL)
                .default_value(saved)
        });
        let key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(KEY_PLACEHOLDER)
                .masked(true)
        });
        let edits = cx.subscribe(&url, |this: &mut Self, field, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = field.read(cx).value().to_string();
                this.edit_gateway_url(&text, cx);
            }
        });
        GatewayUi {
            url,
            key,
            url_error: None,
            _edits: edits,
        }
    }

    fn edit_gateway_url(&mut self, text: &str, cx: &mut Context<Self>) {
        let read = read_url(text);
        if let Some(ui) = self.settings_ui.as_mut() {
            ui.gateway.url_error = read.as_ref().err().copied();
        }
        let url = read.unwrap_or_default();
        if url == self.settings.gateway.url {
            return cx.notify();
        }
        self.settings.gateway.url = url;
        self.save_settings(cx);
        self.check_gateway_soon(cx);
    }

    fn submit_gateway_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.settings_ui.as_ref().map(|ui| ui.gateway.key.clone()) else {
            return;
        };
        let Some(key) = GatewayKey::parse(&field.read(cx).value()) else {
            return;
        };
        if self.settings.gateway.url.is_none() {
            return;
        }
        field.update(cx, |field, cx| field.set_value("", window, cx));
        self.save_gateway_key(key, cx);
    }

    fn gateway_status(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let configured = self.settings.gateway.url.is_some();
        let (mark, connected, state, detail) = match &self.gateway.link {
            GatewayLink::Unset => (
                IconName::Key,
                false,
                UNSET_TITLE.to_owned(),
                UNSET_DETAIL.to_owned(),
            ),
            GatewayLink::Checking => (
                IconName::Loading,
                false,
                CHECKING_TITLE.to_owned(),
                String::new(),
            ),
            GatewayLink::Ready { place, models, .. } => (
                IconName::CheckCircle,
                true,
                format!(
                    "{READY_TITLE}{DETAIL_SEPARATOR}{} {MODELS_SUFFIX}",
                    models.len()
                ),
                place_text(*place).to_owned(),
            ),
            GatewayLink::Failed { error, .. } => (
                IconName::CancelCircle,
                false,
                FAILED_TITLE.to_owned(),
                error.clone(),
            ),
        };
        let has_key = matches!(
            self.gateway.link,
            GatewayLink::Ready { .. }
                | GatewayLink::Failed {
                    access: Some(_),
                    ..
                }
        );
        let mut actions = Vec::new();
        if configured {
            actions.push(
                Button::new(BTN_CHECK)
                    .label(CHECK_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.check_gateway(cx)))
                    .into_any_element(),
            );
        }
        if has_key {
            actions.push(
                Button::new(BTN_FORGET)
                    .label(FORGET_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.forget_gateway_key(cx)))
                    .into_any_element(),
            );
        }
        account_row(
            AccountRow {
                mark,
                name: GATEWAY_NAME,
                state,
                detail,
                connected,
                actions,
                below: None,
            },
            pal,
            cx,
        )
    }

    fn gateway_connection(&self, ui: &GatewayUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let key = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&ui.key).mask_toggle()),
            )
            .child(
                Button::new(BTN_SAVE_KEY)
                    .label(SAVE_KEY_LABEL)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.submit_gateway_key(window, cx);
                    })),
            )
            .into_any_element();
        let address = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(Input::new(&ui.url))
            .children(ui.url_error.map(|error| hint(&error.to_string(), pal)))
            .into_any_element();
        let rows = [
            self.gateway_status(cx),
            card_field(URL_LABEL, address, pal),
            card_field(KEY_LABEL, key, pal),
        ];
        settings_section(
            CONNECTION_TITLE,
            Some(CONNECTION_HINT),
            [panel_card(rows, pal)],
            pal,
        )
    }

    pub(super) fn gateway_pane(&self, ui: &GatewayUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        pane_column([
            self.gateway_connection(ui, cx),
            self.gateway_models_card(cx),
            footnote(KEY_NOTE, pal),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_address_turns_the_gateway_off() {
        assert_eq!(read_url("  "), Ok(None));
        assert_eq!(read_url("gateway.local"), Err(HttpUrlError::NoScheme));
    }
}
