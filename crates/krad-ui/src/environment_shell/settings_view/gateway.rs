mod models;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, FontWeight, IntoElement,
    ParentElement, Styled, Subscription, Window,
};
use krad_domain::{GatewayKey, GatewayUrl, GatewayUrlError, Route};
use krad_store::LoginPlace;

use super::gitlab::page::{field_row, section_heading, status_detail, status_mark, KEYRING_NOTE};
use super::{form_row, hint, panel_card, EnvironmentShell, SettingsPane, CHOICE_PAD};
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::environment_shell::gateway::GatewayLink;
use crate::theme::{palette, ICON_SM, LINE_SM, RADIUS_XL, SPACE_0_5, SPACE_2, SPACE_3, TEXT_SM};

const URL_PLACEHOLDER: &str = "http://127.0.0.1:8317";
const KEY_PLACEHOLDER: &str = "A key from access.api-keys";
const INTRO: &str = "Run CLIProxyAPI, or another Anthropic-compatible gateway, to reach the models of your accounts from Claude Code chats. The app only connects to it; start it and add accounts with its own tools.";
const CONNECTION_TITLE: &str = "Connection";
const URL_LABEL: &str = "Address";
const KEY_LABEL: &str = "Key";
const SAVE_KEY_LABEL: &str = "Save key";
const CHECK_LABEL: &str = "Check again";
const FORGET_LABEL: &str = "Forget key";
const ROUTE_LABEL: &str = "New Claude Code chats use the Gateway";
const ROUTE_HINT: &str = "Chats can still switch route from the model picker.";
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
const ROUTE_SWITCH: &str = "gateway-default-route";
const SECTION_GAP: f32 = 20.0;
const FIELD_GAP: f32 = 10.0;

pub(in crate::environment_shell) struct GatewayUi {
    url: Entity<InputState>,
    key: Entity<InputState>,
    url_error: Option<GatewayUrlError>,
    _edits: Subscription,
}

const fn place_text(place: LoginPlace) -> &'static str {
    match place {
        LoginPlace::Keyring => KEYRING_PLACE,
        LoginPlace::ConfigFolder => CONFIG_FOLDER_PLACE,
    }
}

fn read_url(text: &str) -> Result<Option<GatewayUrl>, GatewayUrlError> {
    match text.parse() {
        Ok(url) => Ok(Some(url)),
        Err(GatewayUrlError::Empty) => Ok(None),
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
                .placeholder(URL_PLACEHOLDER)
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

    fn set_default_route(&mut self, gateway: bool, cx: &mut Context<Self>) {
        self.settings.gateway.default_route = if gateway {
            Route::Gateway
        } else {
            Route::Direct
        };
        self.save_settings(cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn open_gateway_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(window, cx);
        self.select_pane(SettingsPane::Gateway, cx);
    }

    fn gateway_status_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let configured = self.settings.gateway.url.is_some();
        let (mark, success, title, detail) = match &self.gateway.link {
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
        let actions = div()
            .flex()
            .flex_row()
            .flex_none()
            .gap(px(SPACE_2))
            .when(configured, |actions| {
                actions.child(
                    Button::new(BTN_CHECK)
                        .label(CHECK_LABEL)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| this.check_gateway(cx))),
                )
            })
            .when(has_key, |actions| {
                actions.child(
                    Button::new(BTN_FORGET)
                        .label(FORGET_LABEL)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(
                            cx.listener(|this: &mut Self, _, _, cx| this.forget_gateway_key(cx)),
                        ),
                )
            });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_3))
            .p(px(CHOICE_PAD))
            .rounded(px(RADIUS_XL))
            .bg(rgb(pal.bg_panel))
            .child(status_mark(mark, success, pal, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(SPACE_0_5))
                    .child(
                        div()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_strong))
                            .child(title),
                    )
                    .when(!detail.is_empty(), |text| {
                        text.child(status_detail(&detail, success, pal))
                    }),
            )
            .child(actions)
            .into_any_element()
    }

    fn gateway_connection(ui: &GatewayUi, cx: &mut Context<Self>) -> AnyElement {
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
        div()
            .flex()
            .flex_col()
            .gap(px(FIELD_GAP))
            .child(section_heading(CONNECTION_TITLE, None, pal))
            .child(field_row(
                URL_LABEL,
                Input::new(&ui.url).into_any_element(),
                pal,
            ))
            .children(ui.url_error.map(|error| hint(&error.to_string(), pal)))
            .child(field_row(KEY_LABEL, key, pal))
            .into_any_element()
    }

    fn gateway_route_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let routed = self.settings.gateway.default_route == Route::Gateway;
        let toggle = Switch::new(ROUTE_SWITCH)
            .checked(routed)
            .on_click(cx.listener(|this: &mut Self, checked: &bool, _, cx| {
                this.set_default_route(*checked, cx);
            }))
            .into_any_element();
        panel_card([form_row(ROUTE_LABEL, Some(ROUTE_HINT), toggle, pal)], pal)
    }

    pub(super) fn gateway_pane(&self, ui: &GatewayUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(SECTION_GAP))
            .child(hint(INTRO, pal))
            .child(self.gateway_status_card(cx))
            .child(Self::gateway_connection(ui, cx))
            .child(self.gateway_route_card(cx))
            .child(self.gateway_models_card(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(SPACE_3))
                    .child(icon(IconName::Lock, ICON_SM, pal.text_muted))
                    .child(div().flex_1().min_w_0().child(hint(KEYRING_NOTE, pal))),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_address_turns_the_gateway_off_and_a_bad_one_is_explained() {
        assert_eq!(read_url("  "), Ok(None));
        assert_eq!(
            read_url("http://10.0.0.5:8317"),
            Err(GatewayUrlError::PlainHttpToRemote)
        );
        assert!(read_url("http://127.0.0.1:8317").is_ok_and(|url| url.is_some()));
    }
}
