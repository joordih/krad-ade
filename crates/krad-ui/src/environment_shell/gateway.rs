use gpui_kit::{App, Context};
use krad_domain::{AgentPreset, EnvironmentName, GatewayKey, GatewayUrl, TerminalId, ToastKind};
use krad_protocol::{Command, EventKind, GatewayCommand, GatewayState};
use std::path::PathBuf;
use std::time::Duration;

use super::runtime_view::GatewayMirror;
use super::EnvironmentShell;

const CHECK_DELAY: Duration = Duration::from_millis(600);
const KEY_SAVED_TOAST: &str = "Gateway key saved";
const KEY_FORGOTTEN_TOAST: &str = "Gateway key removed";

#[derive(Default)]
pub(super) struct Gateway {
    attempt: u64,
    saving: bool,
}

const fn saved(state: &GatewayState) -> bool {
    !matches!(state, GatewayState::Failed { has_key: false, .. })
}

impl EnvironmentShell {
    pub(super) fn check_gateway(&mut self, cx: &mut Context<Self>) {
        self.check_gateway_after(Duration::ZERO, None, cx);
    }

    pub(super) fn check_gateway_soon(&mut self, cx: &mut Context<Self>) {
        self.check_gateway_after(CHECK_DELAY, None, cx);
    }

    pub(super) fn save_gateway_key(&mut self, key: GatewayKey, cx: &mut Context<Self>) {
        self.check_gateway_after(Duration::ZERO, Some(key), cx);
    }

    pub(super) fn forget_gateway_key(&mut self) {
        let Some(url) = self.runtime.settings.gateway.url.clone() else {
            return;
        };
        self.runtime
            .client
            .post(Command::Gateway(GatewayCommand::ForgetKey { url }));
    }

    fn check_gateway_after(
        &mut self,
        delay: Duration,
        key: Option<GatewayKey>,
        cx: &mut Context<Self>,
    ) {
        self.gateway.attempt += 1;
        let attempt = self.gateway.attempt;
        let Some(url) = self.runtime.settings.gateway.url.clone() else {
            self.gateway.saving = false;
            self.post_gateway_check(None, None);
            return;
        };
        self.gateway.saving = key.is_some();
        self.runtime
            .view
            .update(cx, |model, _| model.gateway.state = GatewayState::Checking);
        cx.notify();
        let timer = cx.background_executor().timer(delay);
        cx.spawn(async move |this, cx| {
            timer.await;
            this.update(cx, |shell, _| {
                if shell.gateway.attempt == attempt {
                    shell.post_gateway_check(Some(url), key);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn post_gateway_check(&self, url: Option<GatewayUrl>, key: Option<GatewayKey>) {
        self.runtime
            .client
            .post(Command::Gateway(GatewayCommand::Check { url, key }));
    }

    pub(super) fn gateway_heard(&mut self, kind: EventKind, cx: &mut Context<Self>) {
        match kind {
            EventKind::GatewayChanged { state } => {
                if state == GatewayState::Checking {
                    return;
                }
                if std::mem::take(&mut self.gateway.saving) && saved(&state) {
                    self.toast(ToastKind::Success, KEY_SAVED_TOAST, cx);
                }
                self.settle_gateway(cx);
            }
            EventKind::GatewayKeyForgotten { error } => {
                match error {
                    None => self.toast(ToastKind::Success, KEY_FORGOTTEN_TOAST, cx),
                    Some(error) => self.toast(ToastKind::Error, &error, cx),
                }
                self.check_gateway(cx);
            }
            _ => {}
        }
    }

    pub(super) fn gateway_replaced(&mut self, cx: &mut App) {
        self.refresh_gateway_offers(cx);
    }

    fn settle_gateway(&mut self, cx: &mut Context<Self>) {
        self.refresh_gateway_offers(cx);
        self.flush_chats_of(AgentPreset::Gateway, cx);
        cx.notify();
    }

    pub(super) fn recheck_failed_gateway(&mut self, cx: &mut Context<Self>) {
        if matches!(self.gateway(cx).state, GatewayState::Failed { .. }) {
            self.check_gateway(cx);
        }
    }

    pub(super) fn gateway<'a>(&self, cx: &'a App) -> &'a GatewayMirror {
        &self.runtime.view.read(cx).gateway
    }

    pub(super) fn refresh_gateway_offers(&self, cx: &mut App) {
        let chosen = &self.runtime.settings.gateway.models;
        self.runtime
            .view
            .update(cx, |model, _| model.gateway.offer(chosen));
    }

    pub(super) fn chat_folder(&self, env: &EnvironmentName, id: TerminalId, cx: &App) -> PathBuf {
        if let Some(placement) = self.cloud_placement_of(env, id) {
            return placement.worktree;
        }
        self.envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone())
            .or_else(|| self.env_home(env, cx))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_key_is_told_unless_the_key_could_not_be_kept() {
        assert!(saved(&GatewayState::Failed {
            has_key: true,
            error: "refused".to_owned(),
        }));
        assert!(!saved(&GatewayState::Failed {
            has_key: false,
            error: "no keyring".to_owned(),
        }));
        assert!(saved(&GatewayState::Unset));
    }
}
