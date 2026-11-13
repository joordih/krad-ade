use krad_domain::{CodeHost, GithubToken, GitlabSignIn, GitlabTokens};
use krad_github::{DeviceLogin as GithubDevice, GithubClient, GithubError};
use krad_gitlab::{
    DeviceLogin as GitlabDevice, GitlabClient, GitlabConfig, GitlabError, LoopbackLogin, OAuthError,
};
use krad_protocol::{LoginState, SignInWith};
use krad_store::{
    delete_github_login, delete_gitlab_login, find_github_login, find_gitlab_login,
    write_github_login, write_gitlab_login,
};

use super::ReviewDone;
use crate::code_hosts::gitlab_token_saver;
use crate::jobs::{JobDone, Steps};

#[derive(Debug)]
pub(crate) enum LoginTokens {
    Gitlab(GitlabConfig, GitlabTokens),
    Github(GithubToken),
}

#[derive(Debug)]
pub(crate) enum LoginDone {
    Checked {
        host: CodeHost,
        check: u64,
        state: Result<LoginState, String>,
    },
    Waiting {
        host: CodeHost,
        attempt: u64,
        state: LoginState,
    },
    Finished {
        host: CodeHost,
        attempt: u64,
        tokens: Result<LoginTokens, String>,
    },
    Kept {
        host: CodeHost,
        kept: Result<String, String>,
    },
    SignedOut {
        host: CodeHost,
        deleted: Result<(), String>,
    },
}

impl From<LoginDone> for JobDone {
    fn from(done: LoginDone) -> Self {
        Self::Review(Box::new(ReviewDone::Login(done)))
    }
}

#[derive(Debug, Default)]
struct Slot {
    attempt: u64,
    check: u64,
    signing_in: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Logins {
    github: Slot,
    gitlab: Slot,
}

impl Logins {
    const fn slot(&mut self, host: CodeHost) -> &mut Slot {
        match host {
            CodeHost::Github => &mut self.github,
            CodeHost::Gitlab => &mut self.gitlab,
        }
    }

    pub(crate) const fn begin(&mut self, host: CodeHost, signing_in: bool) -> u64 {
        let slot = self.slot(host);
        slot.attempt += 1;
        slot.signing_in = signing_in;
        slot.attempt
    }

    pub(crate) const fn is_current(&mut self, host: CodeHost, attempt: u64) -> bool {
        self.slot(host).attempt == attempt
    }

    pub(crate) const fn settle(&mut self, host: CodeHost) {
        self.slot(host).signing_in = false;
    }

    pub(crate) const fn next_check(&mut self, host: CodeHost) -> Option<u64> {
        let slot = self.slot(host);
        if slot.signing_in {
            return None;
        }
        slot.check += 1;
        Some(slot.check)
    }

    pub(crate) const fn is_latest_check(&mut self, host: CodeHost, check: u64) -> bool {
        let slot = self.slot(host);
        !slot.signing_in && slot.check == check
    }
}

#[tracing::instrument(skip_all, fields(?host))]
pub(crate) fn check(host: CodeHost, check: u64, gitlab: Option<GitlabConfig>) -> JobDone {
    let state = match host {
        CodeHost::Github => check_github(),
        CodeHost::Gitlab => gitlab.map_or(Ok(LoginState::SignedOut), check_gitlab),
    };
    LoginDone::Checked { host, check, state }.into()
}

fn check_github() -> Result<LoginState, String> {
    let Some((token, place)) = find_github_login().map_err(|error| error.to_string())? else {
        return Ok(LoginState::SignedOut);
    };
    let username = match GithubClient::new(&token).current_user() {
        Ok(user) => Some(user.username),
        Err(GithubError::SignedOut) => return Ok(LoginState::SignedOut),
        Err(_) => None,
    };
    Ok(LoginState::SignedIn { username, place })
}

fn check_gitlab(config: GitlabConfig) -> Result<LoginState, String> {
    let found = find_gitlab_login(&config.host).map_err(|error| error.to_string())?;
    let Some((tokens, place)) = found else {
        return Ok(LoginState::SignedOut);
    };
    let saver = gitlab_token_saver(config.host.clone());
    let username = match GitlabClient::new(config, tokens, saver).current_user() {
        Ok(user) => Some(user.username),
        Err(GitlabError::Login(OAuthError::Expired)) => return Ok(LoginState::SignedOut),
        Err(_) => None,
    };
    Ok(LoginState::SignedIn { username, place })
}

#[tracing::instrument(skip_all, fields(?host))]
pub(crate) fn sign_in(
    (host, attempt): (CodeHost, u64),
    gitlab: Result<GitlabConfig, String>,
    with: SignInWith,
    steps: &Steps,
) -> JobDone {
    let waiting = |state| {
        let waiting = LoginDone::Waiting {
            host,
            attempt,
            state,
        };
        steps.report(waiting.into());
    };
    let tokens = match with {
        SignInWith::GithubDevice => github_tokens(waiting),
        SignInWith::Gitlab { method } => {
            gitlab.and_then(|config| gitlab_tokens(config, method, waiting))
        }
    };
    LoginDone::Finished {
        host,
        attempt,
        tokens,
    }
    .into()
}

fn github_tokens(waiting: impl Fn(LoginState)) -> Result<LoginTokens, String> {
    let login = GithubDevice::start().map_err(|error| error.to_string())?;
    waiting(LoginState::WaitingForCode {
        user_code: login.user_code.clone(),
        page: login.verification_uri.clone(),
        expires_in: login.expires_in,
    });
    login
        .finish()
        .map(LoginTokens::Github)
        .map_err(|error| error.to_string())
}

fn gitlab_tokens(
    config: GitlabConfig,
    method: GitlabSignIn,
    waiting: impl Fn(LoginState),
) -> Result<LoginTokens, String> {
    let tokens = match method {
        GitlabSignIn::Browser => {
            let login = LoopbackLogin::start(config.clone()).map_err(|error| error.to_string())?;
            waiting(LoginState::WaitingForBrowser {
                page: login.authorize_url(),
            });
            login.finish()
        }
        GitlabSignIn::DeviceCode => {
            let login = GitlabDevice::start(config.clone()).map_err(|error| error.to_string())?;
            waiting(LoginState::WaitingForCode {
                user_code: login.user_code.clone(),
                page: login.verification_uri.clone(),
                expires_in: login.expires_in,
            });
            login.finish()
        }
    };
    tokens
        .map(|tokens| LoginTokens::Gitlab(config, tokens))
        .map_err(|error| error.to_string())
}

#[tracing::instrument(skip_all)]
pub(crate) fn keep(tokens: LoginTokens) -> JobDone {
    let (host, kept) = match tokens {
        LoginTokens::Github(token) => (CodeHost::Github, keep_github(&token)),
        LoginTokens::Gitlab(config, tokens) => (CodeHost::Gitlab, keep_gitlab(config, tokens)),
    };
    LoginDone::Kept { host, kept }.into()
}

fn keep_github(token: &GithubToken) -> Result<String, String> {
    write_github_login(token).map_err(|error| error.to_string())?;
    GithubClient::new(token)
        .current_user()
        .map(|user| user.username)
        .map_err(|error| error.to_string())
}

fn keep_gitlab(config: GitlabConfig, tokens: GitlabTokens) -> Result<String, String> {
    write_gitlab_login(&config.host, &tokens).map_err(|error| error.to_string())?;
    let saver = gitlab_token_saver(config.host.clone());
    GitlabClient::new(config, tokens, saver)
        .current_user()
        .map(|user| user.username)
        .map_err(|error| error.to_string())
}

#[tracing::instrument(skip_all, fields(?host))]
pub(crate) fn sign_out(host: CodeHost, gitlab: Result<GitlabConfig, String>) -> JobDone {
    let deleted = match host {
        CodeHost::Github => delete_github_login().map_err(|error| error.to_string()),
        CodeHost::Gitlab => gitlab.and_then(|config| {
            delete_gitlab_login(&config.host).map_err(|error| error.to_string())
        }),
    };
    LoginDone::SignedOut { host, deleted }.into()
}

pub(crate) fn gitlab_config(
    settings: &krad_domain::GitlabSettings,
) -> Result<GitlabConfig, String> {
    GitlabConfig::try_from(settings).map_err(|error| error.to_string())
}
