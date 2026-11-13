mod device;
pub(super) mod page;
pub(super) mod remotes;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use krad_domain::{CodeHost, GitlabSignIn, LoginPlace, ToastKind};
use krad_gitlab::GitlabConfig;
use krad_protocol::{CodeHostCommand, LoginState, SignInWith};
use std::time::{Duration, Instant};

use super::EnvironmentShell;
use remotes::GroupRemote;

const HOST_PLACEHOLDER: &str = "https://gitlab.com";
const CLIENT_ID_PLACEHOLDER: &str = "Application ID";
const COUNTDOWN_TICK: Duration = Duration::from_secs(1);

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum GitlabStatus {
    Checking,
    SignedOut,
    SignedIn(Connected),
    WaitingForBrowser,
    WaitingForCode(DeviceCode),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct Connected {
    username: Option<String>,
    place: LoginPlace,
    checked_at: Instant,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct DeviceCode {
    user_code: String,
    page: String,
    expires_at: Instant,
}

impl GitlabStatus {
    fn heard(state: &LoginState) -> Option<Self> {
        Some(match state.clone() {
            LoginState::Checking => Self::Checking,
            LoginState::SignedOut => Self::SignedOut,
            LoginState::SignedIn { username, place } => Self::SignedIn(Connected {
                username,
                place,
                checked_at: Instant::now(),
            }),
            LoginState::WaitingForBrowser { .. } => Self::WaitingForBrowser,
            LoginState::WaitingForCode {
                user_code,
                page,
                expires_in,
            } => Self::WaitingForCode(DeviceCode {
                user_code,
                page,
                expires_at: Instant::now() + expires_in,
            }),
            LoginState::Unknown => return None,
        })
    }
}

pub(super) struct GitlabUi {
    host: Entity<InputState>,
    client_id: Entity<InputState>,
    pub(super) status: GitlabStatus,
    remotes: Option<Vec<GroupRemote>>,
    pub(super) unfolded: bool,
    _edits: [Subscription; 2],
}

impl EnvironmentShell {
    pub(super) fn gitlab_ui(&self, window: &mut Window, cx: &mut Context<Self>) -> GitlabUi {
        let saved = self.settings.gitlab.clone();
        let mut field = |value: String, placeholder: &'static str, cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        };
        let host = field(saved.host, HOST_PLACEHOLDER, cx);
        let client_id = field(saved.client_id, CLIENT_ID_PLACEHOLDER, cx);
        let edits = [
            cx.subscribe(&host, |this: &mut Self, field, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.settings.gitlab.host = field.read(cx).value().to_string();
                    this.save_settings(cx);
                }
            }),
            cx.subscribe(
                &client_id,
                |this: &mut Self, field, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.settings.gitlab.client_id = field.read(cx).value().to_string();
                        this.save_settings(cx);
                    }
                },
            ),
        ];
        GitlabUi {
            host,
            client_id,
            status: GitlabStatus::Checking,
            remotes: None,
            unfolded: false,
            _edits: edits,
        }
    }

    fn gitlab_config(&mut self, cx: &mut Context<Self>) -> Option<GitlabConfig> {
        GitlabConfig::try_from(&self.settings.gitlab)
            .map_err(|error| self.toast(ToastKind::Error, &error.to_string(), cx))
            .ok()
    }

    fn gitlab_ui_mut(&mut self) -> Option<&mut GitlabUi> {
        self.settings_ui.as_mut().map(|ui| &mut ui.gitlab)
    }

    pub(super) fn gitlab_heard(&mut self, state: &LoginState, cx: &mut Context<Self>) {
        let Some(status) = GitlabStatus::heard(state) else {
            return;
        };
        match (state, &status) {
            (LoginState::WaitingForBrowser { page }, _) => cx.open_url(page),
            (_, GitlabStatus::WaitingForCode(_)) => Self::tick_device_countdown(cx),
            _ => {}
        }
        if let Some(gitlab) = self.gitlab_ui_mut() {
            gitlab.status = status;
            cx.notify();
        }
    }

    pub(in crate::environment_shell) fn refresh_gitlab_status(&self) {
        let host = CodeHost::Gitlab;
        self.post_code_host(CodeHostCommand::Check { host });
    }

    fn cancel_gitlab_sign_in(&self) {
        let host = CodeHost::Gitlab;
        self.post_code_host(CodeHostCommand::Cancel { host });
    }

    fn sign_in_gitlab(&mut self, method: GitlabSignIn, cx: &mut Context<Self>) {
        if self.gitlab_config(cx).is_none() {
            return;
        }
        let with = SignInWith::Gitlab { method };
        self.post_code_host(CodeHostCommand::SignIn { with });
    }

    fn tick_device_countdown(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(COUNTDOWN_TICK).await;
            let waiting = this
                .update(cx, |shell, cx| {
                    let waiting = shell.gitlab_ui_mut().is_some_and(|gitlab| {
                        matches!(gitlab.status, GitlabStatus::WaitingForCode(_))
                    });
                    if waiting {
                        cx.notify();
                    }
                    waiting
                })
                .unwrap_or_default();
            if !waiting {
                break;
            }
        })
        .detach();
    }

    fn sign_out_gitlab(&mut self, cx: &mut Context<Self>) {
        if self.gitlab_config(cx).is_none() {
            return;
        }
        let host = CodeHost::Gitlab;
        self.post_code_host(CodeHostCommand::SignOut { host });
    }

    fn set_gitlab_sign_in(&mut self, sign_in: GitlabSignIn, cx: &mut Context<Self>) {
        self.settings.gitlab.sign_in = sign_in;
        self.save_settings(cx);
        cx.notify();
    }
}
