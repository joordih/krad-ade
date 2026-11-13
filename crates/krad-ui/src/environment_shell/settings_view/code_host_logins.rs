use gpui_kit::Context;
use krad_domain::{CodeHost, ToastKind};
use krad_protocol::{CodeHostCommand, CodeHostLogin, Command, LoginNews};

use super::EnvironmentShell;

const GITHUB_NAME: &str = "GitHub";
const GITLAB_NAME: &str = "GitLab";

const fn host_name(host: CodeHost) -> &'static str {
    match host {
        CodeHost::Github => GITHUB_NAME,
        CodeHost::Gitlab => GITLAB_NAME,
    }
}

impl EnvironmentShell {
    pub(super) fn post_code_host(&self, command: CodeHostCommand) {
        self.runtime.handle.post(Command::CodeHost(command));
    }

    pub(in crate::environment_shell) fn code_host_login_heard(
        &mut self,
        login: CodeHostLogin,
        cx: &mut Context<Self>,
    ) {
        let CodeHostLogin { host, state, news } = login;
        if let Some(news) = news {
            let name = host_name(host);
            let shown = match news {
                LoginNews::SignedIn { username } => Some((
                    ToastKind::Success,
                    format!("Signed in to {name} as @{username}"),
                )),
                LoginNews::SignedOut => Some((ToastKind::Success, format!("Signed out of {name}"))),
                LoginNews::Failed { message } => Some((ToastKind::Error, message)),
                LoginNews::Unknown => None,
            };
            if let Some((kind, text)) = shown {
                self.toast(kind, &text, cx);
            }
            self.review.forget_login();
        }
        match host {
            CodeHost::Github => self.github_heard(state, cx),
            CodeHost::Gitlab => self.gitlab_heard(&state, cx),
        }
    }
}
