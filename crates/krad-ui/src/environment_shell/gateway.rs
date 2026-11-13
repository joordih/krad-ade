use gpui_kit::Context;
use krad_chat::{GatewayModel, ModelOffer};
use krad_domain::{AgentPreset, EnvironmentName, GatewayKey, ModelId, TerminalId, ToastKind};
use krad_host::HostLink;
use krad_runtime::{GatewayCheck, GatewayChecked};
use krad_store::LoginPlace;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::terminal_card::home_dir;
use super::EnvironmentShell;

const CHECK_DELAY: Duration = Duration::from_millis(600);
const NOT_SET_UP_TEXT: &str =
    "The Gateway is not set up: add its address and key in Settings › Gateway";
const UNREACHABLE_HINT: &str =
    "Check it in Settings › Gateway, or pick a direct model to continue without it";
const NO_MODEL_TEXT: &str = "Pick the Gateway models to offer in Settings › Gateway";
const KEY_SAVED_TOAST: &str = "Gateway key saved";
const KEY_FORGOTTEN_TOAST: &str = "Gateway key removed";

#[derive(Clone, Debug, Default)]
pub(super) enum GatewayLink {
    #[default]
    Unset,
    Checking,
    Ready {
        place: LoginPlace,
        models: Vec<GatewayModel>,
    },
    Failed {
        has_key: bool,
        error: String,
    },
}

impl From<GatewayChecked> for GatewayLink {
    fn from(checked: GatewayChecked) -> Self {
        match checked {
            Ok(None) => Self::Unset,
            Ok(Some(GatewayCheck {
                place,
                models: Ok(models),
            })) => Self::Ready { place, models },
            Ok(Some(GatewayCheck {
                models: Err(error), ..
            })) => Self::Failed {
                has_key: true,
                error,
            },
            Err(error) => Self::Failed {
                has_key: false,
                error,
            },
        }
    }
}

#[derive(Default)]
pub(super) struct Gateway {
    pub(super) link: GatewayLink,
    attempt: u64,
    offers: Vec<ModelOffer>,
}

pub(super) enum GatewayReach {
    Ready,
    Waiting,
    Unavailable(String),
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

    pub(super) fn forget_gateway_key(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.settings.gateway.url.clone() else {
            return;
        };
        let removed = self.runtime.handle.forget_gateway_key(url);
        cx.spawn(async move |this, cx| {
            let removed = removed.await;
            this.update(cx, |shell, cx| {
                match removed {
                    Ok(Ok(())) => shell.toast(ToastKind::Success, KEY_FORGOTTEN_TOAST, cx),
                    Ok(Err(error)) => shell.toast(ToastKind::Error, &error, cx),
                    Err(stopped) => shell.toast(ToastKind::Error, &stopped.to_string(), cx),
                }
                shell.check_gateway(cx);
            })
            .unwrap_or_default();
        })
        .detach();
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
            drop(self.runtime.handle.check_gateway(None, None));
            self.settle_gateway(GatewayLink::Unset, cx);
            return;
        };
        let saving = key.is_some();
        self.gateway.link = GatewayLink::Checking;
        cx.notify();
        let timer = cx.background_executor().timer(delay);
        cx.spawn(async move |this, cx| {
            timer.await;
            let current = this
                .update(cx, |shell, _| shell.gateway.attempt == attempt)
                .unwrap_or_default();
            if !current {
                return;
            }
            let Ok(checked) = this.update(cx, |shell, _| {
                shell.runtime.handle.check_gateway(Some(url), key)
            }) else {
                return;
            };
            let checked = checked.await;
            this.update(cx, |shell, cx| {
                if shell.gateway.attempt != attempt {
                    return;
                }
                let link = checked.map_or_else(
                    |stopped| GatewayLink::Failed {
                        has_key: false,
                        error: stopped.to_string(),
                    },
                    GatewayLink::from,
                );
                if saving && !matches!(link, GatewayLink::Failed { has_key: false, .. }) {
                    shell.toast(ToastKind::Success, KEY_SAVED_TOAST, cx);
                }
                shell.settle_gateway(link, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn settle_gateway(&mut self, link: GatewayLink, cx: &mut Context<Self>) {
        self.gateway.link = link;
        self.refresh_gateway_offers();
        self.flush_chats_of(AgentPreset::Gateway, cx);
        cx.notify();
    }

    pub(super) fn recheck_failed_gateway(&mut self, cx: &mut Context<Self>) {
        if matches!(self.gateway.link, GatewayLink::Failed { .. }) {
            self.check_gateway(cx);
        }
    }

    pub(super) fn gateway_reach(&self) -> GatewayReach {
        match &self.gateway.link {
            GatewayLink::Ready { .. } => GatewayReach::Ready,
            GatewayLink::Checking => GatewayReach::Waiting,
            GatewayLink::Unset => GatewayReach::Unavailable(NOT_SET_UP_TEXT.to_owned()),
            GatewayLink::Failed { error, .. } => {
                GatewayReach::Unavailable(format!("{error}. {UNREACHABLE_HINT}"))
            }
        }
    }

    pub(super) fn gateway_served(&self) -> &[GatewayModel] {
        match &self.gateway.link {
            GatewayLink::Ready { models, .. } => models,
            GatewayLink::Unset | GatewayLink::Checking | GatewayLink::Failed { .. } => &[],
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
        matches!(self.gateway.link, GatewayLink::Ready { .. })
    }

    pub(super) fn gateway_model(&self, chosen: Option<&ModelId>) -> Option<ModelId> {
        chosen.cloned().or_else(|| {
            self.gateway_offers()
                .first()
                .and_then(|offer| ModelId::parse(&offer.value))
        })
    }

    pub(super) const fn no_gateway_model_text() -> &'static str {
        NO_MODEL_TEXT
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
    fn a_check_the_runtime_answers_tells_whether_a_key_is_kept_without_holding_it() {
        let failed = |checked| match GatewayLink::from(checked) {
            GatewayLink::Failed { has_key, .. } => Some(has_key),
            GatewayLink::Unset | GatewayLink::Checking | GatewayLink::Ready { .. } => None,
        };
        let unreachable = Ok(Some(GatewayCheck {
            place: LoginPlace::Keyring,
            models: Err("refused".to_owned()),
        }));
        assert_eq!(failed(unreachable), Some(true));
        assert_eq!(failed(Err("no keyring".to_owned())), Some(false));
        assert!(matches!(GatewayLink::from(Ok(None)), GatewayLink::Unset));
        let ready = GatewayLink::from(Ok(Some(GatewayCheck {
            place: LoginPlace::ConfigFolder,
            models: Ok(Vec::new()),
        })));
        assert!(matches!(
            ready,
            GatewayLink::Ready {
                place: LoginPlace::ConfigFolder,
                ..
            }
        ));
    }
}
