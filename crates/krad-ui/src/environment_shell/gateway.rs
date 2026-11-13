use gpui_kit::Context;
use krad_domain::{
    AgentPreset, EnvironmentName, GatewayKey, GatewayModel, GatewayUrl, ModelOffer, TerminalId,
    ToastKind,
};
use krad_host::HostLink;
use krad_protocol::{Command, EventKind, GatewayCommand, GatewayState};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::terminal_card::home_dir;
use super::EnvironmentShell;

const CHECK_DELAY: Duration = Duration::from_millis(600);
const KEY_SAVED_TOAST: &str = "Gateway key saved";
const KEY_FORGOTTEN_TOAST: &str = "Gateway key removed";

#[derive(Default)]
pub(super) struct Gateway {
    pub(super) link: GatewayState,
    attempt: u64,
    saving: bool,
    offers: Vec<ModelOffer>,
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
        let Some(url) = self.settings.gateway.url.clone() else {
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
        let Some(url) = self.settings.gateway.url.clone() else {
            self.gateway.saving = false;
            self.post_gateway_check(None, None);
            return;
        };
        self.gateway.saving = key.is_some();
        self.gateway.link = GatewayState::Checking;
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
                    self.gateway.link = state;
                    return;
                }
                if std::mem::take(&mut self.gateway.saving) && saved(&state) {
                    self.toast(ToastKind::Success, KEY_SAVED_TOAST, cx);
                }
                self.settle_gateway(state, cx);
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

    pub(super) fn gateway_replaced(&mut self, state: GatewayState) {
        self.gateway.link = state;
        self.refresh_gateway_offers();
    }

    fn settle_gateway(&mut self, link: GatewayState, cx: &mut Context<Self>) {
        self.gateway.link = link;
        self.refresh_gateway_offers();
        self.flush_chats_of(AgentPreset::Gateway, cx);
        cx.notify();
    }

    pub(super) fn recheck_failed_gateway(&mut self, cx: &mut Context<Self>) {
        if matches!(self.gateway.link, GatewayState::Failed { .. }) {
            self.check_gateway(cx);
        }
    }

    pub(super) fn gateway_served(&self) -> &[GatewayModel] {
        match &self.gateway.link {
            GatewayState::Ready { models, .. } => models,
            GatewayState::Unset | GatewayState::Checking | GatewayState::Failed { .. } => &[],
        }
    }

    pub(super) fn refresh_gateway_offers(&mut self) {
        let served = self.gateway_served();
        let offers = self
            .settings
            .gateway
            .models
            .iter()
            .filter_map(|model| {
                served
                    .iter()
                    .find(|served| served.offer.value == model.as_str())
                    .map(|served| served.offer.clone())
            })
            .collect();
        self.gateway.offers = offers;
    }

    pub(super) fn gateway_offers(&self) -> &[ModelOffer] {
        &self.gateway.offers
    }

    pub(super) fn gateway_ready(&self) -> bool {
        matches!(self.gateway.link, GatewayState::Ready { .. })
    }

    pub(super) fn chat_folder(&self, env: &EnvironmentName, id: TerminalId) -> PathBuf {
        if let Some(placement) = self.cloud_placement_of(env, id) {
            return placement.worktree;
        }
        self.envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone())
            .or_else(|| match self.env_link(env) {
                HostLink::Remote(ssh) => ssh.home().map(Path::to_path_buf),
                HostLink::Local => home_dir().map(PathBuf::from),
            })
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
