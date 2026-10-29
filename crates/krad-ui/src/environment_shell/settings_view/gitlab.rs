mod device;
pub(super) mod page;
pub(super) mod remotes;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use krad_domain::{GitlabSignIn, GitlabTokens};
use krad_gitlab::{
    DeviceLogin, GitlabClient, GitlabConfig, GitlabError, LoopbackLogin, OAuthError, TokenSaver,
};
use krad_store::{delete_gitlab_login, find_gitlab_login, write_gitlab_login, LoginPlace};
use std::time::{Duration, Instant};

use super::EnvironmentShell;
use remotes::GroupRemote;

const HOST_PLACEHOLDER: &str = "https://gitlab.com";
const CLIENT_ID_PLACEHOLDER: &str = "Application ID";
const SIGNED_OUT_TOAST: &str = "Signed out of GitLab";
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

impl DeviceCode {
    fn new(login: &DeviceLogin) -> Self {
        Self {
            user_code: login.user_code.clone(),
            page: login.verification_uri.clone(),
            expires_at: Instant::now() + login.expires_in,
        }
    }
}

pub(super) struct GitlabUi {
    host: Entity<InputState>,
    client_id: Entity<InputState>,
    pub(super) status: GitlabStatus,
    attempt: u64,
    remotes: Option<Vec<GroupRemote>>,
    pub(super) unfolded: bool,
    _edits: [Subscription; 2],
}

pub(in crate::environment_shell) fn gitlab_token_saver(host: String) -> TokenSaver {
    Box::new(move |tokens| write_gitlab_login(&host, tokens).map_err(std::io::Error::other))
}

fn keep_login(config: GitlabConfig, tokens: GitlabTokens) -> Result<String, String> {
    write_gitlab_login(&config.host, &tokens).map_err(|error| error.to_string())?;
    let host = config.host.clone();
    GitlabClient::new(config, tokens, gitlab_token_saver(host))
        .current_user()
        .map(|user| user.username)
        .map_err(|error| error.to_string())
}

fn check_login(config: GitlabConfig) -> Result<GitlabStatus, String> {
    let Some((tokens, place)) = find_gitlab_login(&config.host).map_err(|e| e.to_string())? else {
        return Ok(GitlabStatus::SignedOut);
    };
    let host = config.host.clone();
    let username = match GitlabClient::new(config, tokens, gitlab_token_saver(host)).current_user()
    {
        Ok(user) => Some(user.username),
        Err(GitlabError::Login(OAuthError::Expired)) => return Ok(GitlabStatus::SignedOut),
        Err(_) => None,
    };
    Ok(GitlabStatus::SignedIn(Connected {
        username,
        place,
        checked_at: Instant::now(),
    }))
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
            attempt: 0,
            remotes: None,
            unfolded: false,
            _edits: edits,
        }
    }

    fn gitlab_config(&mut self, cx: &mut Context<Self>) -> Option<GitlabConfig> {
        GitlabConfig::try_from(&self.settings.gitlab)
            .map_err(|error| self.toast(&error.to_string(), cx))
            .ok()
    }

    fn gitlab_ui_mut(&mut self) -> Option<&mut GitlabUi> {
        self.settings_ui.as_mut().map(|ui| &mut ui.gitlab)
    }

    fn set_gitlab_status(&mut self, status: GitlabStatus, cx: &mut Context<Self>) {
        if let Some(gitlab) = self.gitlab_ui_mut() {
            gitlab.status = status;
            cx.notify();
        }
    }

    fn set_attempt_status(&mut self, attempt: u64, status: GitlabStatus, cx: &mut Context<Self>) {
        if self
            .gitlab_ui_mut()
            .is_some_and(|gitlab| gitlab.attempt == attempt)
        {
            self.set_gitlab_status(status, cx);
        }
    }

    fn next_sign_in_attempt(&mut self) -> Option<u64> {
        let gitlab = self.gitlab_ui_mut()?;
        gitlab.attempt += 1;
        Some(gitlab.attempt)
    }

    pub(in crate::environment_shell) fn refresh_gitlab_status(&mut self, cx: &mut Context<Self>) {
        let Ok(config) = GitlabConfig::try_from(&self.settings.gitlab) else {
            self.set_gitlab_status(GitlabStatus::SignedOut, cx);
            return;
        };
        self.set_gitlab_status(GitlabStatus::Checking, cx);
        let check = cx.background_spawn(async move { check_login(config) });
        cx.spawn(async move |this, cx| {
            let checked = check.await;
            this.update(cx, |shell, cx| {
                let status = checked.unwrap_or_else(|error| {
                    shell.toast(&error, cx);
                    GitlabStatus::SignedOut
                });
                shell.set_gitlab_status(status, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn cancel_gitlab_sign_in(&mut self, cx: &mut Context<Self>) {
        if self.next_sign_in_attempt().is_some() {
            self.refresh_gitlab_status(cx);
        }
    }

    fn sign_in_gitlab(&mut self, sign_in: GitlabSignIn, cx: &mut Context<Self>) {
        let Some(config) = self.gitlab_config(cx) else {
            return;
        };
        let Some(attempt) = self.next_sign_in_attempt() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let start_config = config.clone();
            let outcome = match sign_in {
                GitlabSignIn::Browser => {
                    let started = background
                        .spawn(async move { LoopbackLogin::start(start_config) })
                        .await;
                    match started {
                        Ok(login) => {
                            let page = login.authorize_url();
                            this.update(cx, |shell, cx| {
                                cx.open_url(&page);
                                shell.set_attempt_status(
                                    attempt,
                                    GitlabStatus::WaitingForBrowser,
                                    cx,
                                );
                            })
                            .unwrap_or_default();
                            background.spawn(async move { login.finish() }).await
                        }
                        Err(error) => Err(error),
                    }
                }
                GitlabSignIn::DeviceCode => {
                    let started = background
                        .spawn(async move { DeviceLogin::start(start_config) })
                        .await;
                    match started {
                        Ok(login) => {
                            let code = GitlabStatus::WaitingForCode(DeviceCode::new(&login));
                            this.update(cx, |shell, cx| {
                                shell.set_attempt_status(attempt, code, cx);
                                Self::tick_device_countdown(attempt, cx);
                            })
                            .unwrap_or_default();
                            background.spawn(async move { login.finish() }).await
                        }
                        Err(error) => Err(error),
                    }
                }
            };
            let current = this
                .update(cx, |shell, _| {
                    shell
                        .gitlab_ui_mut()
                        .is_some_and(|gitlab| gitlab.attempt == attempt)
                })
                .unwrap_or_default();
            if !current {
                return;
            }
            let kept = match outcome {
                Ok(tokens) => {
                    background
                        .spawn(async move { keep_login(config, tokens) })
                        .await
                }
                Err(error) => Err(error.to_string()),
            };
            this.update(cx, |shell, cx| {
                match kept {
                    Ok(username) => shell.toast(&format!("Signed in to GitLab as @{username}"), cx),
                    Err(text) => shell.toast(&text, cx),
                }
                shell.refresh_gitlab_status(cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn tick_device_countdown(attempt: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(COUNTDOWN_TICK).await;
            let waiting = this
                .update(cx, |shell, cx| {
                    let waiting = shell.gitlab_ui_mut().is_some_and(|gitlab| {
                        gitlab.attempt == attempt
                            && matches!(gitlab.status, GitlabStatus::WaitingForCode(_))
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
        let Some(config) = self.gitlab_config(cx) else {
            return;
        };
        let delete = cx.background_spawn(async move { delete_gitlab_login(&config.host) });
        cx.spawn(async move |this, cx| {
            let deleted = delete.await;
            this.update(cx, |shell, cx| {
                match deleted {
                    Ok(()) => shell.toast(SIGNED_OUT_TOAST, cx),
                    Err(error) => shell.toast(&error.to_string(), cx),
                }
                shell.refresh_gitlab_status(cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn set_gitlab_sign_in(&mut self, sign_in: GitlabSignIn, cx: &mut Context<Self>) {
        self.settings.gitlab.sign_in = sign_in;
        self.save_settings(cx);
        cx.notify();
    }
}
