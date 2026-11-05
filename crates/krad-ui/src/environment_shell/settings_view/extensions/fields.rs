use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::{div, px, AnyElement, Context, IntoElement, ParentElement, Styled};
use krad_domain::ExtensionSettingValue;
use krad_extensions::api::manifest::{ExtensionManifest, SettingKind};
use krad_extensions::api::protocol::SettingValue;

use super::super::super::extensions::default_value;
use super::super::{form_row, EnvironmentShell};
use super::{text_of, FieldSlot, CHOICE_PREFIX, FIELD_W, SETTING_TOGGLE_PREFIX};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, SPACE_2};

impl EnvironmentShell {
    pub(super) fn extension_setting_rows(
        &self,
        manifest: &ExtensionManifest,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        let values = self.extension_values(&super::super::super::extensions::SlotKey {
            extension: manifest.id.clone(),
            environment: None,
        });
        let fields = self.settings_ui.as_ref().map(|ui| &ui.extensions.fields);
        manifest
            .contributes
            .settings
            .iter()
            .enumerate()
            .map(|(index, setting)| {
                let current = values
                    .get(&setting.key)
                    .cloned()
                    .unwrap_or_else(|| default_value(&setting.kind));
                let control = match &setting.kind {
                    SettingKind::Toggle { .. } => {
                        let on = matches!(current, SettingValue::Toggle(true));
                        let (extension, key) = (manifest.id.clone(), setting.key.clone());
                        Switch::new((SETTING_TOGGLE_PREFIX, index))
                            .checked(on)
                            .on_click(cx.listener(move |this: &mut Self, checked: &bool, _, cx| {
                                this.set_extension_value(
                                    &extension,
                                    &key,
                                    ExtensionSettingValue::Toggle(*checked),
                                    cx,
                                );
                            }))
                            .into_any_element()
                    }
                    SettingKind::Choice { options, .. } => {
                        let chosen = text_of(&current);
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(SPACE_2))
                            .children(options.iter().enumerate().map(|(option_index, option)| {
                                let (extension, key, value) = (
                                    manifest.id.clone(),
                                    setting.key.clone(),
                                    option.value.clone(),
                                );
                                Button::new((CHOICE_PREFIX, index * options.len() + option_index))
                                    .label(option.label.clone())
                                    .small()
                                    .variant(if option.value == chosen {
                                        ButtonVariant::Primary
                                    } else {
                                        ButtonVariant::Secondary
                                    })
                                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                        this.set_extension_value(
                                            &extension,
                                            &key,
                                            ExtensionSettingValue::Text(value.clone()),
                                            cx,
                                        );
                                    }))
                            }))
                            .into_any_element()
                    }
                    SettingKind::Text { .. } | SettingKind::Number { .. } => {
                        let slot =
                            FieldSlot::Setting(manifest.id.to_string(), setting.key.to_string());
                        fields.and_then(|fields| fields.get(&slot)).map_or_else(
                            || div().into_any_element(),
                            |field| {
                                div()
                                    .w(px(FIELD_W))
                                    .child(Input::new(field))
                                    .into_any_element()
                            },
                        )
                    }
                };
                form_row(&setting.title, setting.description.as_deref(), control, pal)
            })
            .collect()
    }

    pub(super) fn extension_secret_rows(
        &self,
        manifest: &ExtensionManifest,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        let fields = self.settings_ui.as_ref().map(|ui| &ui.extensions.fields);
        manifest
            .contributes
            .secrets
            .iter()
            .map(|secret| {
                let slot = FieldSlot::Secret(manifest.id.to_string(), secret.name.to_string());
                let control = fields.and_then(|fields| fields.get(&slot)).map_or_else(
                    || div().into_any_element(),
                    |field| {
                        div()
                            .w(px(FIELD_W))
                            .child(Input::new(field))
                            .into_any_element()
                    },
                );
                form_row(&secret.title, secret.description.as_deref(), control, pal)
            })
            .collect()
    }
}
