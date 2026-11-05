mod values;

use gpui_kit::Context;
use krad_domain::ExtensionSettingValue;
use krad_extensions::api::limits::TOAST_INTERVAL;
use krad_extensions::api::manifest::SettingKind;
use krad_extensions::api::protocol::{
    CloseView, Empty, GetSettings, GroupInfo, OpenFile, OpenFileParams, OpenTerminal, OpenView,
    Request, RunCommand, SettingValue, SettingsValues, WorkspaceContext, WorkspaceContextResult,
};
use krad_extensions::api::view::Tone;
use krad_extensions::program::{AppCall, AppNotice, CallId};
use krad_lsp::lsp_types::Position;
use std::collections::BTreeMap;

use super::super::EnvironmentShell;
use super::prompts::Prompt;
use super::SlotKey;

const NO_SUCH_COMMAND: &str = "no enabled extension declares that command";
const NO_SUCH_VIEW: &str = "the extension declares no such view";
const WARNING_MARK: &str = "⚠";
const ERROR_MARK: &str = "✕";
const SUCCESS_MARK: &str = "✓";

impl EnvironmentShell {
    pub(super) fn answer_call<R: Request>(&self, key: &SlotKey, id: CallId, result: &R::Result) {
        let Some(connection) = self.extensions.connection(key) else {
            return;
        };
        if let Err(error) = connection.answer::<R>(id, result) {
            tracing::debug!(%error, "extension answer not delivered");
        }
    }

    pub(super) fn refuse_call(&self, key: &SlotKey, id: CallId, reason: &str) {
        let Some(connection) = self.extensions.connection(key) else {
            return;
        };
        if let Err(error) = connection.refuse(id, reason) {
            tracing::debug!(%error, "extension refusal not delivered");
        }
    }

    fn extension_permissions(
        &self,
        key: &SlotKey,
    ) -> Vec<krad_extensions::api::manifest::Permission> {
        self.extensions
            .record(&key.extension)
            .and_then(super::ExtensionRecord::program)
            .and_then(|program| program.manifest().program.clone())
            .map_or_else(Vec::new, |declared| declared.permissions)
    }

    pub(super) fn handle_call(
        &mut self,
        key: &SlotKey,
        id: CallId,
        call: AppCall,
        cx: &mut Context<Self>,
    ) {
        let permissions = self.extension_permissions(key);
        if let Err(refusal) = krad_extensions::program::allowed(&permissions, &call) {
            self.refuse_call(key, id, &refusal.to_string());
            return;
        }
        match call {
            AppCall::Confirm(params) => {
                self.queue_extension_prompt(Prompt::Confirm(key.clone(), id, params), cx);
            }
            AppCall::Pick(params) => {
                self.queue_extension_prompt(Prompt::Pick(key.clone(), id, params), cx);
            }
            AppCall::InputBox(params) => {
                self.queue_extension_prompt(Prompt::Input(key.clone(), id, params), cx);
            }
            AppCall::OpenTerminal(params) => match self.open_extension_terminal(key, params, cx) {
                Ok(handle) => self.answer_call::<OpenTerminal>(key, id, &handle),
                Err(refusal) => self.refuse_call(key, id, refusal.reason()),
            },
            AppCall::WorkspaceContext => {
                let context = self.workspace_context(key);
                self.answer_call::<WorkspaceContext>(key, id, &context);
            }
            AppCall::OpenFile(OpenFileParams { path, position }) => {
                let at = position.map(|at| Position::new(at.line, at.column));
                self.open_file_at(&path, at, cx);
                self.answer_call::<OpenFile>(key, id, &Empty {});
            }
            AppCall::GetSettings => {
                let values = self.extension_values(key);
                self.answer_call::<GetSettings>(key, id, &values);
            }
            AppCall::GetSecret(params) => self.read_extension_secret(key, id, &params, cx),
            AppCall::SetSecret(params) => self.write_extension_secret(key, id, params, cx),
            AppCall::RunCommand(params) => {
                if self.declares_command(&params.command) {
                    self.run_extension_command(params.command, params.target, cx);
                    self.answer_call::<RunCommand>(key, id, &Empty {});
                } else {
                    self.refuse_call(key, id, NO_SUCH_COMMAND);
                }
            }
            AppCall::OpenView(params) => {
                let opened = self.open_extension_view(
                    &key.extension,
                    &params.view.view,
                    params.view.instance,
                    params.title,
                    cx,
                );
                if opened {
                    self.answer_call::<OpenView>(key, id, &Empty {});
                } else {
                    self.refuse_call(key, id, NO_SUCH_VIEW);
                }
            }
            AppCall::CloseView(view) => {
                self.close_extension_views(key, &view, cx);
                self.answer_call::<CloseView>(key, id, &Empty {});
            }
        }
    }

    pub(super) fn handle_notice(
        &mut self,
        key: &SlotKey,
        notice: AppNotice,
        cx: &mut Context<Self>,
    ) {
        let permissions = self.extension_permissions(key);
        if let Err(refusal) = krad_extensions::program::allowed_notice(&permissions, &notice) {
            tracing::debug!(extension = %key.extension, %refusal, "extension notice refused");
            return;
        }
        let title = self.extensions.title_of(&key.extension);
        match notice {
            AppNotice::Log(params) => {
                if let Some(record) = self.extensions.record(&key.extension) {
                    record.log.push(params.message);
                }
            }
            AppNotice::PatchView(params) => self.patch_extension_views(key, &params, cx),
            AppNotice::Toast(params) => {
                let now = std::time::Instant::now();
                let last = self.extensions.toast_times.get(key).copied();
                if !toast_allowed(last, now) {
                    tracing::debug!(extension = %key.extension, "extension toast dropped");
                    return;
                }
                self.extensions.toast_times.insert(key.clone(), now);
                let text = match tone_mark(params.tone) {
                    Some(mark) => format!("{mark} {title}: {}", params.message),
                    None => format!("{title}: {}", params.message),
                };
                self.push_toast(text);
            }
            AppNotice::Progress(params) => self.track_progress(key, &title, params),
            AppNotice::SendTerminal(params) => {
                self.send_extension_terminal(key, params.terminal, params.text);
            }
        }
    }

    fn workspace_context(&self, key: &SlotKey) -> WorkspaceContextResult {
        let env = key
            .environment
            .clone()
            .unwrap_or_else(|| self.active.clone());
        let live = self.envs.get(&env);
        let groups = live.map_or_else(Vec::new, |live| {
            live.data
                .groups
                .iter()
                .map(|group| GroupInfo {
                    id: group.id.as_u64().to_string(),
                    name: group.name.clone(),
                    root: group.root.clone(),
                })
                .collect()
        });
        let active_file = live
            .and_then(|live| live.focused.and_then(|id| live.data.open_file_of(id)))
            .map(|file| file.path.clone());
        WorkspaceContextResult {
            environment: self.environment_info(&env),
            groups,
            active_file,
            cursor: None,
        }
    }

    pub(in crate::environment_shell) fn extension_values(&self, key: &SlotKey) -> SettingsValues {
        let Some(manifest) = self
            .extensions
            .record(&key.extension)
            .and_then(super::ExtensionRecord::manifest)
        else {
            return BTreeMap::new();
        };
        let stored = self.extensions.values.of(&key.extension);
        manifest
            .contributes
            .settings
            .iter()
            .map(|setting| {
                let value = stored
                    .get(setting.key.as_ref())
                    .map_or_else(|| default_value(&setting.kind), setting_value);
                (setting.key.clone(), value)
            })
            .collect()
    }
}

fn toast_allowed(last: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= TOAST_INTERVAL)
}

const fn tone_mark(tone: Tone) -> Option<&'static str> {
    match tone {
        Tone::Warning => Some(WARNING_MARK),
        Tone::Error => Some(ERROR_MARK),
        Tone::Success => Some(SUCCESS_MARK),
        Tone::Neutral | Tone::Info => None,
    }
}

pub(in crate::environment_shell) fn default_value(kind: &SettingKind) -> SettingValue {
    match kind {
        SettingKind::Text { default } | SettingKind::Choice { default, .. } => {
            SettingValue::Text(default.clone())
        }
        SettingKind::Toggle { default } => SettingValue::Toggle(*default),
        SettingKind::Number { default, .. } => SettingValue::Number(*default),
    }
}

fn setting_value(value: &ExtensionSettingValue) -> SettingValue {
    match value {
        ExtensionSettingValue::Toggle(on) => SettingValue::Toggle(*on),
        ExtensionSettingValue::Number(number) => SettingValue::Number(*number),
        ExtensionSettingValue::Text(text) => SettingValue::Text(text.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extensions::api::manifest::SettingOption;

    #[test]
    fn toasts_from_one_extension_are_spaced_out() {
        let start = std::time::Instant::now();
        assert!(toast_allowed(None, start));
        assert!(!toast_allowed(Some(start), start));
        assert!(toast_allowed(Some(start), start + TOAST_INTERVAL));
    }

    #[test]
    fn declared_defaults_and_stored_values_read_as_protocol_values() {
        let choice = SettingKind::Choice {
            options: vec![SettingOption {
                value: "fast".to_owned(),
                label: "Fast".to_owned(),
            }],
            default: "fast".to_owned(),
        };
        assert_eq!(
            default_value(&choice),
            SettingValue::Text("fast".to_owned())
        );
        assert_eq!(
            default_value(&SettingKind::Toggle { default: true }),
            SettingValue::Toggle(true)
        );
        assert_eq!(
            setting_value(&ExtensionSettingValue::Number(2.5)),
            SettingValue::Number(2.5)
        );
    }

    #[test]
    fn toasts_mark_only_the_tones_that_need_attention() {
        assert_eq!(tone_mark(Tone::Error), Some(ERROR_MARK));
        assert_eq!(tone_mark(Tone::Neutral), None);
    }
}
