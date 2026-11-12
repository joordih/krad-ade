use gpui_kit::{AppContext as _, Context};
use krad_domain::{ExtensionSecret, ExtensionSecretKey, ExtensionSettingValue, ToastKind};
use krad_extensions::api::protocol::{
    Empty, GetSecret, GetSecretParams, Secret, SetSecret, SetSecretParams,
};
use krad_extensions::api::SettingKey;
use krad_extensions::program::CallId;

use super::super::super::EnvironmentShell;
use super::super::SlotKey;

const NO_SUCH_SECRET: &str = "the extension declares no such secret";
const SECRET_STORE_FAILED: &str = "the secret store failed";

impl EnvironmentShell {
    pub(super) fn declares_secret(
        &self,
        key: &SlotKey,
        name: &krad_extensions::api::SecretName,
    ) -> bool {
        self.extensions
            .record(&key.extension)
            .and_then(super::super::ExtensionRecord::manifest)
            .is_some_and(|manifest| {
                manifest
                    .contributes
                    .secrets
                    .iter()
                    .any(|secret| secret.name == *name)
            })
    }

    pub(super) fn read_extension_secret(
        &mut self,
        key: &SlotKey,
        id: CallId,
        params: &GetSecretParams,
        cx: &mut Context<Self>,
    ) {
        if !self.declares_secret(key, &params.name) {
            self.refuse_call(key, id, NO_SUCH_SECRET);
            return;
        }
        let secret_key = ExtensionSecretKey::new(&key.extension, params.name.as_ref());
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

    pub(super) fn write_extension_secret(
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
        let secret_key = ExtensionSecretKey::new(&key.extension, params.name.as_ref());
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
        self.extensions.values.set(extension, key.as_ref(), value);
        self.write_extension_values(cx);
        self.announce_settings(extension);
        cx.notify();
    }

    pub(in crate::environment_shell) fn forget_extension_data(
        &mut self,
        extension: &krad_extensions::api::ExtensionId,
        cx: &mut Context<Self>,
    ) {
        let names: Vec<String> = self
            .extensions
            .record(extension)
            .and_then(super::super::ExtensionRecord::manifest)
            .map_or_else(Vec::new, |manifest| {
                manifest
                    .contributes
                    .secrets
                    .iter()
                    .map(|secret| secret.name.to_string())
                    .collect()
            });
        let keys: Vec<ExtensionSecretKey> = names
            .iter()
            .map(|name| ExtensionSecretKey::new(extension, name))
            .collect();
        cx.background_spawn(async move {
            for key in keys {
                if let Err(error) = krad_store::delete_extension_secret(&key) {
                    tracing::warn!(%error, "an extension secret could not be deleted");
                }
            }
        })
        .detach();
        self.extensions.values.forget(extension);
        self.write_extension_values(cx);
    }

    fn write_extension_values(&mut self, cx: &mut Context<Self>) {
        let values = self.extensions.values.clone();
        let writing =
            cx.background_spawn(async move { krad_store::write_extension_settings(&values) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = writing.await {
                this.update(cx, |shell, cx| {
                    shell.toast(ToastKind::Error, &error.to_string(), cx);
                })
                .unwrap_or_default();
            }
        })
        .detach();
    }
}
