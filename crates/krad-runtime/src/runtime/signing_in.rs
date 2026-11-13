use krad_domain::CodeHost;
use krad_protocol::{
    CodeHostCommand, CodeHostLogin, ErrorCode, EventKind, LoginNews, LoginState, Reply, SignInWith,
};

use std::io;

use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::jobs::JobDone;
use crate::reviews::{check_login, gitlab_config, keep_login, sign_in, sign_out, LoginDone};

const NOT_STARTED: &str = "the sign-in could not start";

impl Runtime {
    pub(super) fn execute_code_host(&mut self, command: CodeHostCommand) -> Reply {
        let (host, started) = match command {
            CodeHostCommand::Check { host } => {
                self.check_login(host);
                (host, Ok(()))
            }
            CodeHostCommand::Cancel { host } => {
                self.reviews.logins.begin(host, false);
                self.check_login(host);
                (host, Ok(()))
            }
            CodeHostCommand::SignIn { with } => (with.host(), self.start_sign_in(with)),
            CodeHostCommand::SignOut { host } => {
                self.reviews.logins.begin(host, false);
                let gitlab = gitlab_config(&self.settings.current.gitlab);
                (host, self.jobs.run(move || sign_out(host, gitlab)))
            }
            CodeHostCommand::Unknown => {
                return refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND);
            }
        };
        match started {
            Ok(()) => Reply::Accepted,
            Err(error) => {
                tracing::warn!(%error, "a code host sign-in could not start");
                self.reviews.logins.settle(host);
                refused(ErrorCode::Unavailable, NOT_STARTED)
            }
        }
    }

    fn start_sign_in(&mut self, with: SignInWith) -> io::Result<()> {
        let host = with.host();
        let attempt = self.reviews.logins.begin(host, true);
        let gitlab = gitlab_config(&self.settings.current.gitlab);
        self.jobs.run_reporting(move |steps| {
            steps.report(sign_in((host, attempt), gitlab, with, steps));
        })
    }

    fn check_login(&mut self, host: CodeHost) {
        let Some(check) = self.reviews.logins.next_check(host) else {
            return;
        };
        self.show_login(host, LoginState::Checking, None);
        let gitlab = gitlab_config(&self.settings.current.gitlab).ok();
        if let Err(error) = self.jobs.run(move || check_login(host, check, gitlab)) {
            tracing::warn!(%error, "a code host login check could not start");
            self.show_login(host, LoginState::SignedOut, None);
        }
    }

    fn show_login(&mut self, host: CodeHost, state: LoginState, news: Option<LoginNews>) {
        self.outbox.push(EventKind::CodeHostLogin(CodeHostLogin {
            host,
            state,
            news,
        }));
    }

    pub(super) fn login_done(&mut self, done: LoginDone) {
        match done {
            LoginDone::Checked { host, check, state } => {
                if !self.reviews.logins.is_latest_check(host, check) {
                    return;
                }
                match state {
                    Ok(state) => self.show_login(host, state, None),
                    Err(message) => {
                        let news = Some(LoginNews::Failed { message });
                        self.show_login(host, LoginState::SignedOut, news);
                    }
                }
            }
            LoginDone::Waiting {
                host,
                attempt,
                state,
            } => {
                if self.reviews.logins.is_current(host, attempt) {
                    self.show_login(host, state, None);
                }
            }
            LoginDone::Finished {
                host,
                attempt,
                tokens,
            } => {
                if !self.reviews.logins.is_current(host, attempt) {
                    return;
                }
                self.reviews.logins.settle(host);
                match tokens {
                    Ok(tokens) => self.keep_tokens(host, move || keep_login(tokens)),
                    Err(message) => self.login_settled(host, LoginNews::Failed { message }),
                }
            }
            LoginDone::Kept { host, kept } => {
                self.reviews.forget_host(host);
                let news = match kept {
                    Ok(username) => LoginNews::SignedIn { username },
                    Err(message) => LoginNews::Failed { message },
                };
                self.login_settled(host, news);
            }
            LoginDone::SignedOut { host, deleted } => {
                self.reviews.forget_host(host);
                let news = match deleted {
                    Ok(()) => LoginNews::SignedOut,
                    Err(message) => LoginNews::Failed { message },
                };
                self.login_settled(host, news);
            }
        }
    }

    fn keep_tokens(&mut self, host: CodeHost, keep: impl FnOnce() -> JobDone + Send + 'static) {
        if let Err(error) = self.jobs.run(keep) {
            tracing::warn!(%error, "a code host login could not be kept");
            let message = NOT_STARTED.to_owned();
            self.login_settled(host, LoginNews::Failed { message });
        }
    }

    fn login_settled(&mut self, host: CodeHost, news: LoginNews) {
        self.show_login(host, LoginState::Checking, Some(news));
        self.check_login(host);
    }
}
