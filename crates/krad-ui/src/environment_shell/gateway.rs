use gpui_kit::{AppContext as _, Context};
use krad_chat::{gateway_models, ModelOffer};
use krad_domain::{
    AgentPreset, EnvironmentName, GatewayAccess, GatewayKey, GatewayUrl, ModelId, TerminalId,
};
use krad_store::{delete_gateway_key, find_gateway_key, write_gateway_key, LoginPlace};
use std::path::PathBuf;
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
        access: GatewayAccess,
        place: LoginPlace,
        models: Vec<ModelOffer>,
    },
    Failed {
        access: Option<GatewayAccess>,
        error: String,
    },
}

#[derive(Default)]
pub(super) struct Gateway {
    pub(super) link: GatewayLink,
    attempt: u64,
    offers: Vec<ModelOffer>,
}

pub(super) enum GatewayReach {
    Ready(GatewayAccess),
    Waiting,
    Unavailable(String),
}

struct Checked {
    access: GatewayAccess,
    place: LoginPlace,
    models: Result<Vec<ModelOffer>, String>,
}

fn read_access(url: &GatewayUrl) -> Result<Option<(GatewayAccess, LoginPlace)>, String> {
    let found = find_gateway_key(url).map_err(|error| error.to_string())?;
    Ok(found.map(|(key, place)| {
        (
            GatewayAccess {
                url: url.clone(),
                key,
            },
            place,
        )
    }))
}

fn check(url: &GatewayUrl, key: Option<&GatewayKey>) -> Result<Option<Checked>, String> {
    if let Some(key) = key {
        write_gateway_key(url, key).map_err(|error| error.to_string())?;
    }
    let Some((access, place)) = read_access(url)? else {
        return Ok(None);
    };
    let models = gateway_models(&access).map_err(|error| error.to_string());
    Ok(Some(Checked {
        access,
        place,
        models,
    }))
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
        let removed = cx.background_spawn(async move { delete_gateway_key(&url) });
        cx.spawn(async move |this, cx| {
            let removed = removed.await;
            this.update(cx, |shell, cx| {
                match removed {
                    Ok(()) => shell.toast(KEY_FORGOTTEN_TOAST, cx),
                    Err(error) => shell.toast(&error.to_string(), cx),
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
            let checked = cx
                .background_executor()
                .spawn(async move { check(&url, key.as_ref()) })
                .await;
            this.update(cx, |shell, cx| {
                if shell.gateway.attempt != attempt {
                    return;
                }
                let link = match checked {
                    Ok(None) => GatewayLink::Unset,
                    Ok(Some(Checked {
                        access,
                        place,
                        models: Ok(models),
                    })) => GatewayLink::Ready {
                        access,
                        place,
                        models,
                    },
                    Ok(Some(Checked {
                        access,
                        models: Err(error),
                        ..
                    })) => GatewayLink::Failed {
                        access: Some(access),
                        error,
                    },
                    Err(error) => GatewayLink::Failed {
                        access: None,
                        error,
                    },
                };
                if saving && !matches!(link, GatewayLink::Failed { access: None, .. }) {
                    shell.toast(KEY_SAVED_TOAST, cx);
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
            GatewayLink::Ready { access, .. } => GatewayReach::Ready(access.clone()),
            GatewayLink::Checking => GatewayReach::Waiting,
            GatewayLink::Unset => GatewayReach::Unavailable(NOT_SET_UP_TEXT.to_owned()),
            GatewayLink::Failed { error, .. } => {
                GatewayReach::Unavailable(format!("{error}. {UNREACHABLE_HINT}"))
            }
        }
    }

    pub(super) fn gateway_served(&self) -> &[ModelOffer] {
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
            .models()
            .iter()
            .filter_map(|model| {
                served
                    .iter()
                    .find(|offer| offer.value == model.as_str())
                    .cloned()
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
        self.envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone())
            .or_else(|| home_dir().map(PathBuf::from))
            .unwrap_or_default()
    }
}
