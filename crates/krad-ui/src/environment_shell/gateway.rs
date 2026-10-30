use gpui_kit::{AppContext as _, Context};
use krad_chat::{gateway_env, gateway_models, ModelOffer};
use krad_domain::{
    AgentPreset, EnvironmentName, GatewayAccess, GatewayKey, GatewayUrl, ModelId, Route, TerminalId,
};
use krad_store::{delete_gateway_key, find_gateway_key, write_gateway_key, LoginPlace};
use std::path::PathBuf;
use std::time::Duration;

use super::sessions::Launch;
use super::EnvironmentShell;

const CHECK_DELAY: Duration = Duration::from_millis(600);
const NOT_SET_UP_TEXT: &str =
    "The Gateway is not set up: add its address and key in Settings › Gateway";
const UNREACHABLE_HINT: &str =
    "Check it in Settings › Gateway, or pick a direct model to continue without it";
const NO_MODEL_TEXT: &str = "Pick the Gateway models to offer in Settings › Gateway";
const TERMINAL_NOT_STARTED: &str = "was not started";
const TERMINAL_UNCHECKED: &str = "started, but the Gateway did not answer";
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

pub(super) struct WaitingLaunch {
    env: EnvironmentName,
    id: TerminalId,
    launch: Launch,
    cwd: Option<PathBuf>,
}

#[derive(Default)]
pub(super) struct Gateway {
    pub(super) link: GatewayLink,
    attempt: u64,
    waiting: Vec<WaitingLaunch>,
}

pub(super) struct RoutedLaunch {
    pub(super) launch: Launch,
    pub(super) cwd: Option<PathBuf>,
    pub(super) vars: Vec<(String, String)>,
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
        for waiting in std::mem::take(&mut self.gateway.waiting) {
            if self.awaits_session(&waiting.env, waiting.id) {
                self.start_session(&waiting.env, waiting.id, waiting.launch, waiting.cwd, &[]);
            }
        }
        self.flush_chats_of(AgentPreset::ClaudeCode, cx);
        cx.notify();
    }

    fn awaits_session(&self, env: &EnvironmentName, id: TerminalId) -> bool {
        self.envs.get(env).is_some_and(|live| {
            !live.sessions.contains_key(&id) && live.data.nodes.iter().any(|node| node.id == id)
        })
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

    pub(super) fn gateway_offers(&self) -> &[ModelOffer] {
        match &self.gateway.link {
            GatewayLink::Ready { models, .. } => models,
            GatewayLink::Unset | GatewayLink::Checking | GatewayLink::Failed { .. } => &[],
        }
    }

    pub(super) fn gateway_offered(&self) -> bool {
        self.settings.gateway.url.is_some()
    }

    pub(super) fn routed_model(&self, chosen: Option<&ModelId>) -> Option<ModelId> {
        chosen
            .cloned()
            .or_else(|| self.settings.gateway.models().into_iter().next())
    }

    pub(super) const fn no_gateway_model_text() -> &'static str {
        NO_MODEL_TEXT
    }

    pub(super) fn terminal_route(&self, env: &EnvironmentName, id: TerminalId) -> Route {
        self.envs
            .get(env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .map_or(Route::Direct, |node| node.route)
    }

    pub(super) fn routed_launch(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        launch: Launch,
        cwd: Option<PathBuf>,
    ) -> Option<RoutedLaunch> {
        let routed = launch
            .harness()
            .filter(|harness| self.terminal_route(env, id).for_harness(*harness) == Route::Gateway);
        let Some(harness) = routed else {
            return Some(RoutedLaunch {
                launch,
                cwd,
                vars: Vec::new(),
            });
        };
        if matches!(self.gateway.link, GatewayLink::Checking) {
            self.gateway
                .waiting
                .retain(|waiting| waiting.env != *env || waiting.id != id);
            self.gateway.waiting.push(WaitingLaunch {
                env: env.clone(),
                id,
                launch,
                cwd,
            });
            return None;
        }
        let model = self.routed_model(launch.model().as_ref());
        let (access, warning) = match &self.gateway.link {
            GatewayLink::Ready { access, .. } => (access.clone(), None),
            GatewayLink::Failed {
                access: Some(access),
                ..
            } => (access.clone(), Some(TERMINAL_UNCHECKED)),
            GatewayLink::Unset
            | GatewayLink::Checking
            | GatewayLink::Failed { access: None, .. } => {
                self.push_toast(format!(
                    "{} {TERMINAL_NOT_STARTED}: {NOT_SET_UP_TEXT}",
                    harness.display_name()
                ));
                return Some(RoutedLaunch {
                    launch: Launch::Preset(AgentPreset::Shell),
                    cwd,
                    vars: Vec::new(),
                });
            }
        };
        if let Some(warning) = warning {
            self.push_toast(format!("{} {warning}", harness.display_name()));
        }
        Some(RoutedLaunch {
            launch,
            cwd,
            vars: gateway_env(&access, model.as_ref()),
        })
    }
}
