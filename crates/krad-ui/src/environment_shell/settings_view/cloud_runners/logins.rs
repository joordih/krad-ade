use gpui_kit::{div, px, AnyElement, Context, IntoElement, ParentElement, SharedString, Styled};
use krad_domain::CloudRunnerId;

use super::super::super::cloud::{RunnerCli, RunnerLogin};
use super::super::{form_row, EnvironmentShell};
use crate::components::{Button, ButtonVariant};
use crate::theme::{Palette, SPACE_2};

const LOGINS_LABEL: &str = "Sign-ins on the runner";
const CHECKING: &str = "Checking…";
const SIGNED_IN: &str = "Signed in";
const SIGNED_OUT: &str = "Needs sign-in";
const MISSING: &str = "Not installed";
const UNKNOWN: &str = "Unknown";
const GROUP_SEPARATOR: &str = " · ";
const NAME_SEPARATOR: &str = ", ";
const LABEL_SEPARATOR: &str = ": ";
const SIGN_IN_PREFIX: &str = "Sign in to";
const RECHECK_LABEL: &str = "Check again";
const SIGN_IN_ID: &str = "cloud-runner-sign-in";
const RECHECK_PREFIX: &str = "cloud-runner-recheck";

const fn login_label(login: RunnerLogin) -> &'static str {
    match login {
        RunnerLogin::Checking => CHECKING,
        RunnerLogin::SignedIn => SIGNED_IN,
        RunnerLogin::SignedOut => SIGNED_OUT,
        RunnerLogin::Missing => MISSING,
        RunnerLogin::Unknown => UNKNOWN,
    }
}

const ORDER: [RunnerLogin; 5] = [
    RunnerLogin::SignedOut,
    RunnerLogin::SignedIn,
    RunnerLogin::Checking,
    RunnerLogin::Unknown,
    RunnerLogin::Missing,
];

fn logins_summary(logins: &[(RunnerCli, RunnerLogin)]) -> String {
    ORDER
        .iter()
        .filter_map(|wanted| {
            let names: Vec<&str> = logins
                .iter()
                .filter(|(_, login)| login == wanted)
                .map(|(cli, _)| cli.label())
                .collect();
            (!names.is_empty()).then(|| {
                format!(
                    "{}{LABEL_SEPARATOR}{}",
                    login_label(*wanted),
                    names.join(NAME_SEPARATOR)
                )
            })
        })
        .collect::<Vec<_>>()
        .join(GROUP_SEPARATOR)
}

impl EnvironmentShell {
    pub(super) fn runner_logins_row(
        &self,
        runner: CloudRunnerId,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let logins = self.runner_logins(runner);
        if logins.is_empty() {
            return None;
        }
        let sign_ins = logins
            .iter()
            .filter(|(_, login)| login.needs_sign_in())
            .map(|&(cli, _)| {
                Button::new(SharedString::from(format!(
                    "{SIGN_IN_ID}-{}-{}",
                    runner.0,
                    cli.label()
                )))
                .label(format!("{SIGN_IN_PREFIX} {}", cli.label()))
                .small()
                .variant(ButtonVariant::Primary)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.open_runner_sign_in(runner, cli, cx);
                }))
            });
        let controls = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_2))
            .children(sign_ins)
            .child(
                Button::new((RECHECK_PREFIX, runner.0))
                    .label(RECHECK_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.check_runner_logins(runner, cx);
                    })),
            )
            .into_any_element();
        Some(form_row(
            LOGINS_LABEL,
            Some(&logins_summary(&logins)),
            controls,
            pal,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_puts_what_needs_a_sign_in_first() {
        let logins = [
            (RunnerCli::Claude, RunnerLogin::SignedIn),
            (RunnerCli::Codex, RunnerLogin::SignedOut),
            (RunnerCli::Github, RunnerLogin::SignedIn),
            (RunnerCli::Gitlab, RunnerLogin::Missing),
        ];
        assert_eq!(
            logins_summary(&logins),
            "Needs sign-in: Codex · Signed in: Claude Code, GitHub CLI · Not installed: GitLab CLI"
        );
        assert_eq!(logins_summary(&[]), "");
    }
}
