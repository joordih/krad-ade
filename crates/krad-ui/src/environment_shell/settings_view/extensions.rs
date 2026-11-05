mod browse;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement,
    SharedString, Styled, Subscription, Window,
};
use krad_domain::{ExtensionSecret, ExtensionSecretKey, ExtensionSettingValue};
use krad_extensions::api::manifest::{ExtensionManifest, SettingContribution, SettingKind};
use krad_extensions::api::protocol::SettingValue;
use krad_extensions::api::ExtensionId;
use krad_extensions::program::Trust;
use std::collections::HashMap;

use super::super::extensions::{default_value, ClashWith, ExtensionRecord, Loaded, Slot};
use super::sections::{card_field, empty_row, pane_column, settings_section};
use super::{form_row, hint, panel_card, EnvironmentShell, SettingsPane};
use crate::components::{Button, ButtonVariant};
use crate::theme::{mono_family, palette, LINE_XS, SPACE_2, TEXT_XS};

const INSTALLED_TITLE: &str = "Installed";
const INSTALLED_HINT: &str =
    "Extensions add languages, and their own programs can add commands, views and tools.";
const SHORTCUTS_TITLE: &str = "Skipped shortcuts";
const SHORTCUTS_HINT: &str =
    "These shortcuts clash with the app's or another extension's and are not active.";
const ADD_EXTENSION_LABEL: &str = "Add";
const EXTENSIONS_EMPTY: &str = "No extensions yet.";
const STATE_LABEL: &str = "State";
const LOG_LABEL: &str = "Program log";
const MANIFESTS_ONLY: &str = "Languages and toolchains only";
const DISABLED: &str = "Disabled";
const NEEDS_TRUST: &str = "Needs your trust before its program runs";
const RUNNING: &str = "Running";
const STARTING: &str = "Starting…";
const IDLE: &str = "Starts when first used";
const FAILED: &str = "Failed";
const UNSUPPORTED: &str = "Not supported on SSH hosts yet";
const CANNOT_USE: &str = "Can't be used";
const TRUST_LABEL: &str = "Review trust…";
const RETRY_LABEL: &str = "Retry";
const SECRET_PLACEHOLDER: &str = "Type and press Enter to save";
const SECRET_SAVED: &str = "Secret saved";
const CLASH_APP: &str = "the app";
const CLASH_UNREADABLE: &str = "an unreadable shortcut";
const CLASH_PREFIX: &str = "clashes with";
const LOG_SHOWN: usize = 12;
const TRUST_PREFIX: &str = "extension-review-trust";
const RETRY_PREFIX: &str = "extension-retry";
const SETTING_TOGGLE_PREFIX: &str = "extension-setting-toggle";
const CHOICE_PREFIX: &str = "extension-setting-choice";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum FieldSlot {
    Setting(String, String),
    Secret(String, String),
}

#[derive(Default)]
pub(in crate::environment_shell) struct ExtensionsUi {
    browse: browse::IndexBrowser,
    fields: HashMap<FieldSlot, Entity<InputState>>,
    subscriptions: Vec<Subscription>,
}

fn text_of(value: &SettingValue) -> String {
    match value {
        SettingValue::Toggle(on) => on.to_string(),
        SettingValue::Number(number) => number.to_string(),
        SettingValue::Text(text) => text.clone(),
    }
}

fn wanted_fields(manifest: &ExtensionManifest) -> Vec<(FieldSlot, bool)> {
    let id = manifest.id.to_string();
    let settings = manifest
        .contributes
        .settings
        .iter()
        .filter(|setting| {
            matches!(
                setting.kind,
                SettingKind::Text { .. } | SettingKind::Number { .. }
            )
        })
        .map(|setting| {
            (
                FieldSlot::Setting(id.clone(), setting.key.to_string()),
                false,
            )
        });
    let secrets = manifest
        .contributes
        .secrets
        .iter()
        .map(|secret| (FieldSlot::Secret(id.clone(), secret.name.to_string()), true));
    settings.chain(secrets).collect()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_extension_fields(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_index_browser(window, cx);
        let open = self
            .settings_ui
            .as_ref()
            .is_some_and(|ui| ui.pane == SettingsPane::Extensions);
        if !open {
            return;
        }
        let manifests: Vec<ExtensionManifest> = self
            .extensions
            .records
            .iter()
            .filter_map(|record| record.manifest().cloned())
            .collect();
        for manifest in manifests {
            let values = self.extension_values(&super::super::extensions::SlotKey {
                extension: manifest.id.clone(),
                environment: None,
            });
            for (slot, masked) in wanted_fields(&manifest) {
                let known = self
                    .settings_ui
                    .as_ref()
                    .is_some_and(|ui| ui.extensions.fields.contains_key(&slot));
                if known {
                    continue;
                }
                let initial = match &slot {
                    FieldSlot::Setting(_, key) => values
                        .iter()
                        .find(|(setting, _)| setting.to_string() == *key)
                        .map_or_else(String::new, |(_, value)| text_of(value)),
                    FieldSlot::Secret(..) => String::new(),
                };
                let field = cx.new(|cx| {
                    let state = InputState::new(window, cx)
                        .default_value(initial)
                        .masked(masked);
                    if masked {
                        state.placeholder(SECRET_PLACEHOLDER)
                    } else {
                        state
                    }
                });
                let watched = slot.clone();
                let subscription = cx.subscribe_in(
                    &field,
                    window,
                    move |this: &mut Self, state, event: &InputEvent, window, cx| {
                        this.extension_field_event(&watched, state, event, window, cx);
                    },
                );
                if let Some(ui) = self.settings_ui.as_mut() {
                    ui.extensions.fields.insert(slot, field);
                    ui.extensions.subscriptions.push(subscription);
                }
            }
        }
    }

    fn extension_field_event(
        &mut self,
        slot: &FieldSlot,
        state: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = state.read(cx).value().to_string();
        match (slot, event) {
            (FieldSlot::Setting(extension, key), InputEvent::Change) => {
                let Some(setting) = self.setting_of(extension, key) else {
                    return;
                };
                let value = match setting.kind {
                    SettingKind::Number { .. } => match text.trim().parse::<f64>() {
                        Ok(number) => ExtensionSettingValue::Number(number),
                        Err(_) => return,
                    },
                    _ => ExtensionSettingValue::Text(text),
                };
                if let Ok(extension) = extension.parse::<ExtensionId>() {
                    self.set_extension_value(&extension, &setting.key, value, cx);
                }
            }
            (FieldSlot::Secret(extension, name), InputEvent::PressEnter { .. }) => {
                let key = ExtensionSecretKey {
                    extension: extension.clone(),
                    name: name.clone(),
                };
                let secret = ExtensionSecret::new(text);
                state.update(cx, |field, cx| field.set_value("", window, cx));
                let writing = cx.background_spawn(async move {
                    krad_store::write_extension_secret(&key, &secret)
                });
                cx.spawn(async move |this, cx| {
                    let written = writing.await;
                    this.update(cx, |shell, cx| match written {
                        Ok(()) => shell.toast(SECRET_SAVED, cx),
                        Err(error) => shell.toast(&error.to_string(), cx),
                    })
                    .unwrap_or_default();
                })
                .detach();
            }
            _ => {}
        }
    }

    fn setting_of(&self, extension: &str, key: &str) -> Option<SettingContribution> {
        let extension = extension.parse::<ExtensionId>().ok()?;
        self.extensions
            .record(&extension)?
            .manifest()?
            .contributes
            .settings
            .iter()
            .find(|setting| setting.key.as_ref() == key)
            .cloned()
    }

    fn extension_state(&self, record: &ExtensionRecord) -> String {
        let installed = match &record.loaded {
            Loaded::ManifestsOnly => return MANIFESTS_ONLY.to_owned(),
            Loaded::Problem(problem) => return format!("{CANNOT_USE}: {problem}"),
            Loaded::Program(installed) => installed,
        };
        if !record.entry.enabled {
            return DISABLED.to_owned();
        }
        if record.trust() == Trust::Asked {
            return NEEDS_TRUST.to_owned();
        }
        match self.extensions.slot_state(&installed.manifest.id) {
            Some(Slot::Running(_)) => RUNNING.to_owned(),
            Some(Slot::Starting(_) | Slot::Initializing(..) | Slot::WaitingForTools(_)) => {
                STARTING.to_owned()
            }
            Some(Slot::Failed(error)) => format!("{FAILED}: {error}"),
            Some(Slot::Unsupported) => UNSUPPORTED.to_owned(),
            None => IDLE.to_owned(),
        }
    }

    fn extension_card(
        &self,
        index: usize,
        record: &ExtensionRecord,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mut rows = vec![Self::extension_row(index, &record.entry, cx)];
        let failed = record.manifest().is_some_and(|manifest| {
            matches!(
                self.extensions.slot_state(&manifest.id),
                Some(Slot::Failed(_) | Slot::Unsupported)
            )
        });
        let trust_entry = record.entry.id.clone();
        let retry_id = record.manifest().map(|manifest| manifest.id.clone());
        let controls = div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .children((record.trust() == Trust::Asked).then(|| {
                Button::new((TRUST_PREFIX, index))
                    .label(TRUST_LABEL)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.review_trust(&trust_entry, cx);
                    }))
            }))
            .children(retry_id.filter(|_| failed).map(|id| {
                Button::new((RETRY_PREFIX, index))
                    .label(RETRY_LABEL)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.retry_extension(&id, cx);
                    }))
            }))
            .into_any_element();
        let state = self.extension_state(record);
        rows.push(form_row(STATE_LABEL, Some(&state), controls, pal));
        if let Some(manifest) = record.manifest() {
            rows.extend(self.extension_setting_rows(manifest, cx));
            rows.extend(self.extension_secret_rows(manifest, cx));
        }
        let lines = record.log.lines();
        if !lines.is_empty() {
            let shown = lines[lines.len().saturating_sub(LOG_SHOWN)..].join("\n");
            let mono = mono_family(&self.settings.terminal_font);
            rows.push(card_field(
                LOG_LABEL,
                div()
                    .font_family(mono)
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_subtle))
                    .child(SharedString::from(shown))
                    .into_any_element(),
                pal,
            ));
        }
        panel_card(rows, pal)
    }

    fn extension_setting_rows(
        &self,
        manifest: &ExtensionManifest,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        let values = self.extension_values(&super::super::extensions::SlotKey {
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

    fn extension_secret_rows(
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

    pub(super) fn extensions_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mut cards: Vec<AnyElement> = self
            .extensions
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| self.extension_card(index, record, cx))
            .collect();
        let untracked: Vec<AnyElement> = self
            .settings
            .extensions
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                !self
                    .extensions
                    .records
                    .iter()
                    .any(|record| record.entry.id == entry.id)
            })
            .map(|(index, entry)| {
                Self::extension_row(index + self.extensions.records.len(), entry, cx)
            })
            .collect();
        if !untracked.is_empty() {
            cards.push(panel_card(untracked, pal));
        }
        if cards.is_empty() {
            cards.push(panel_card([empty_row(EXTENSIONS_EMPTY, pal)], pal));
        }
        if let Some(ui) = self.settings_ui.as_ref() {
            cards.push(panel_card(
                [card_field(
                    ADD_EXTENSION_LABEL,
                    Self::extension_adder(&ui.languages, cx),
                    pal,
                )],
                pal,
            ));
        }
        cards.push(hint(&Self::manifests_hint(), pal));
        let mut sections = vec![settings_section(
            INSTALLED_TITLE,
            Some(INSTALLED_HINT),
            cards,
            pal,
        )];
        if !self.extensions.conflicts.is_empty() {
            let rows: Vec<AnyElement> = self
                .extensions
                .conflicts
                .iter()
                .map(|conflict| {
                    let with = match &conflict.with {
                        ClashWith::App => CLASH_APP.to_owned(),
                        ClashWith::Extension(owner) => {
                            format!("{}.{}", owner.extension, owner.command)
                        }
                        ClashWith::Unreadable => CLASH_UNREADABLE.to_owned(),
                    };
                    let label = format!("{} · {}", conflict.keys, conflict.command.command);
                    let detail = format!("{CLASH_PREFIX} {with}");
                    form_row(&label, Some(&detail), div().into_any_element(), pal)
                })
                .collect();
            sections.push(settings_section(
                SHORTCUTS_TITLE,
                Some(SHORTCUTS_HINT),
                [panel_card(rows, pal)],
                pal,
            ));
        }
        sections.extend(self.index_sections(cx));
        pane_column(sections)
    }
}

const FIELD_W: f32 = 240.0;
