use gpui_kit::{AppContext as _, Context};
use krad_domain::{AgentPreset, ExtensionSecret, ExtensionSecretKey, ExtensionSettingValue};
use krad_extensions::api::manifest::SettingKind;
use krad_extensions::api::protocol::{
    CloseView, Empty, GetSecret, GetSecretParams, GetSettings, GroupInfo, OpenFile, OpenFileParams,
    OpenTerminal, OpenTerminalParams, OpenView, ProgressState, Request, RunCommand, Secret,
    SetSecret, SetSecretParams, SettingValue, SettingsValues, TerminalZone, WorkspaceContext,
    WorkspaceContextResult,
};
use krad_extensions::api::view::Tone;
use krad_extensions::api::{SettingKey, TerminalHandle};
use krad_extensions::program::{AppCall, AppNotice, CallId};
use krad_lsp::lsp_types::Position;
use krad_session::PtyInput;
use std::collections::BTreeMap;

use super::super::sessions::Launch;
use super::super::EnvironmentShell;
use super::prompts::Prompt;
use super::SlotKey;

const NO_SUCH_COMMAND: &str = "no enabled extension declares that command";
const NO_SUCH_VIEW: &str = "the extension declares no such view";
const NO_SUCH_SECRET: &str = "the extension declares no such secret";
const NO_TERMINAL: &str = "the terminal could not be opened";
const SECRET_STORE_FAILED: &str = "the secret store failed";
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

    pub(super) fn handle_call(
        &mut self,
        key: &SlotKey,
        id: CallId,
        call: AppCall,
        cx: &mut Context<Self>,
    ) {
        match call {
            AppCall::Confirm(params) => {
                self.queue_extension_prompt(Prompt::Confirm(key.clone(), id, params), cx)
            }
            AppCall::Pick(params) => {
                self.queue_extension_prompt(Prompt::Pick(key.clone(), id, params), cx)
            }
            AppCall::InputBox(params) => {
                self.queue_extension_prompt(Prompt::Input(key.clone(), id, params), cx);
            }
            AppCall::OpenTerminal(params) => match self.open_extension_terminal(key, params, cx) {
                Some(handle) => self.answer_call::<OpenTerminal>(key, id, &handle),
                None => self.refuse_call(key, id, NO_TERMINAL),
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
            AppCall::GetSecret(params) => self.read_extension_secret(key, id, params, cx),
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
        let title = self
            .extensions
            .record(&key.extension)
            .map(super::ExtensionRecord::title)
            .unwrap_or_default();
        match notice {
            AppNotice::Log(params) => {
                if let Some(record) = self.extensions.record(&key.extension) {
                    record.log.push(params.message);
                }
            }
            AppNotice::PatchView(params) => self.patch_extension_views(key, params, cx),
            AppNotice::Toast(params) => {
                let text = match tone_mark(params.tone) {
                    Some(mark) => format!("{mark} {title}: {}", params.message),
                    None => format!("{title}: {}", params.message),
                };
                self.push_toast(text);
            }
            AppNotice::Progress(params) => self.track_progress(key, &title, params),
            AppNotice::SendTerminal(params) => {
                self.send_extension_terminal(params.terminal, params.text)
            }
        }
    }

    fn track_progress(
        &mut self,
        key: &SlotKey,
        title: &str,
        params: krad_extensions::api::protocol::ProgressParams,
    ) {
        let token = params.token.clone();
        let progress = &mut self.extensions.progress;
        progress.retain(|(owner, known, _)| !(owner == key && *known == token));
        match &params.state {
            ProgressState::Begin { title: what } => {
                self.activity_log.record(format!("{title}: {what}"));
                self.extensions.progress.push((key.clone(), token, params));
            }
            ProgressState::Report { .. } => {
                let bounded = krad_extensions::api::protocol::ProgressParams {
                    token: params.token.clone(),
                    state: params.state.clone().bounded(),
                };
                self.extensions.progress.push((key.clone(), token, bounded));
            }
            ProgressState::End => {}
        }
    }

    fn open_extension_terminal(
        &mut self,
        key: &SlotKey,
        params: OpenTerminalParams,
        cx: &mut Context<Self>,
    ) -> Option<TerminalHandle> {
        let env = key
            .environment
            .clone()
            .unwrap_or_else(|| self.active.clone());
        let launch = match params.command {
            Some(command) => Launch::Command {
                program: command.program,
                args: command.args,
                vars: command.env.into_iter().collect(),
            },
            None => Launch::Preset(AgentPreset::Shell),
        };
        let terminal = self.spawn_terminal_in(&env, None, &params.title, launch, None, cx)?;
        if params.zone == TerminalZone::Utilities && env == self.active {
            self.dock_in_activity(terminal, cx);
        }
        self.extensions.next_terminal += 1;
        let handle = TerminalHandle(self.extensions.next_terminal);
        self.extensions.terminals.insert(handle, (env, terminal));
        Some(handle)
    }

    fn send_extension_terminal(&mut self, handle: TerminalHandle, text: String) {
        let Some((env, terminal)) = self.extensions.terminals.get(&handle).cloned() else {
            return;
        };
        let Some(session) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.sessions.get_mut(&terminal))
        else {
            return;
        };
        if session
            .writer_tx
            .send(PtyInput::Bytes(text.into_bytes()))
            .is_err()
        {
            tracing::debug!("extension terminal writer closed");
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
        let stored = self.extensions.values.of(key.extension.as_ref());
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

    fn declares_secret(&self, key: &SlotKey, name: &krad_extensions::api::SecretName) -> bool {
        self.extensions
            .record(&key.extension)
            .and_then(super::ExtensionRecord::manifest)
            .is_some_and(|manifest| {
                manifest
                    .contributes
                    .secrets
                    .iter()
                    .any(|secret| secret.name == *name)
            })
    }

    fn read_extension_secret(
        &mut self,
        key: &SlotKey,
        id: CallId,
        params: GetSecretParams,
        cx: &mut Context<Self>,
    ) {
        if !self.declares_secret(key, &params.name) {
            self.refuse_call(key, id, NO_SUCH_SECRET);
            return;
        }
        let secret_key = ExtensionSecretKey {
            extension: key.extension.to_string(),
            name: params.name.to_string(),
        };
        let reading = cx.background_spawn(async move {
            krad_store::find_extension_secret(&secret_key)
                .map(|found| found.map(|(secret, _)| Secret::new(secret.expose().to_owned())))
        });
        let key = key.clone();
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            this.update(cx, |shell, _| match read {
                Ok(secret) => shell.answer_call::<GetSecret>(&key, id, &secret),
                Err(_) => shell.refuse_call(&key, id, SECRET_STORE_FAILED),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn write_extension_secret(
        &mut self,
        key: &SlotKey,
        id: CallId,
        params: SetSecretParams,
        cx: &mut Context<Self>,
    ) {
        if !self.declares_secret(key, &params.name) {
            self.refuse_call(key, id, NO_SUCH_SECRET);
            return;
        }
        let secret_key = ExtensionSecretKey {
            extension: key.extension.to_string(),
            name: params.name.to_string(),
        };
        let writing = cx.background_spawn(async move {
            match params.value {
                Some(value) => krad_store::write_extension_secret(
                    &secret_key,
                    &ExtensionSecret::new(value.expose().to_owned()),
                ),
                None => krad_store::delete_extension_secret(&secret_key),
            }
        });
        let key = key.clone();
        cx.spawn(async move |this, cx| {
            let written = writing.await;
            this.update(cx, |shell, _| match written {
                Ok(()) => shell.answer_call::<SetSecret>(&key, id, &Empty {}),
                Err(_) => shell.refuse_call(&key, id, SECRET_STORE_FAILED),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn set_extension_value(
        &mut self,
        extension: &krad_extensions::api::ExtensionId,
        key: &SettingKey,
        value: ExtensionSettingValue,
        cx: &mut Context<Self>,
    ) {
        self.extensions
            .values
            .set(extension.as_ref(), key.as_ref(), value);
        let values = self.extensions.values.clone();
        let writing =
            cx.background_spawn(async move { krad_store::write_extension_settings(&values) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = writing.await {
                this.update(cx, |shell, cx| shell.toast(&error.to_string(), cx))
                    .unwrap_or_default();
            }
        })
        .detach();
        self.announce_settings(extension);
        cx.notify();
    }
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
        SettingKind::Text { default } => SettingValue::Text(default.clone()),
        SettingKind::Toggle { default } => SettingValue::Toggle(*default),
        SettingKind::Number { default, .. } => SettingValue::Number(*default),
        SettingKind::Choice { default, .. } => SettingValue::Text(default.clone()),
    }
}

fn setting_value(value: &ExtensionSettingValue) -> SettingValue {
    match value {
        ExtensionSettingValue::Toggle(on) => SettingValue::Toggle(*on),
        ExtensionSettingValue::Number(number) => SettingValue::Number(*number),
        ExtensionSettingValue::Text(text) => SettingValue::Text(text.clone()),
    }
}
