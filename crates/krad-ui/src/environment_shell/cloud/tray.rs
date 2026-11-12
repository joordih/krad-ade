use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{AgentPreset, CloudRunnerId, TerminalId};

use super::super::ask::{tray, tray_button};
use super::super::workers::LeadEnding;
use super::super::EnvironmentShell;
use super::stops::REMOVED_RUNNER;
use super::RunnerCli;
use crate::components::{icon, IconName};
use crate::theme::{palette, ICON, SPACE_2};

const CLOSE_PREFIX: &str = "cloud-tray-close";
const SIGN_IN_PREFIX: &str = "cloud-tray-sign-in";
const RECHECK_PREFIX: &str = "cloud-tray-recheck";
const CLOSE_LABEL: &str = "Close session";
const SIGN_IN_LABEL: &str = "Sign in on runner";
const RECHECK_LABEL: &str = "Check again";
const SIGNED_OUT_MIDDLE: &str = "is not signed in on";

enum CloudTray {
    Removed,
    SignedOut(CloudRunnerId, RunnerCli),
}

fn element_id(prefix: &str, id: TerminalId) -> ElementId {
    ElementId::Name(format!("{prefix}-{}", id.as_u64()).into())
}

impl EnvironmentShell {
    fn cloud_tray_need(&self, id: TerminalId, harness: AgentPreset) -> Option<CloudTray> {
        let runner = self.cloud_placement_of(&self.active, id)?.runner;
        if self.runner_removed_from_settings(runner) {
            return Some(CloudTray::Removed);
        }
        let cli = RunnerCli::of_harness(harness)?;
        self.runner_login(runner, cli)
            .filter(|login| login.needs_sign_in())
            .map(|_| CloudTray::SignedOut(runner, cli))
    }

    pub(in crate::environment_shell) fn cloud_chat_tray(
        &self,
        id: TerminalId,
        harness: AgentPreset,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let need = self.cloud_tray_need(id, harness)?;
        let pal = palette(cx);
        let (text, buttons): (String, Vec<AnyElement>) = match need {
            CloudTray::Removed => {
                let session = self
                    .envs
                    .get(&self.active)
                    .and_then(|live| live.data.session_of(id))?;
                let close = tray_button(element_id(CLOSE_PREFIX, id), true, pal)
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.ask_end_lead(LeadEnding::CloseSession(session), window, cx);
                    }))
                    .child(CLOSE_LABEL)
                    .into_any_element();
                (REMOVED_RUNNER.to_owned(), vec![close])
            }
            CloudTray::SignedOut(runner, cli) => {
                let sign_in = tray_button(element_id(SIGN_IN_PREFIX, id), true, pal)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.open_runner_sign_in(runner, cli, cx);
                    }))
                    .child(SIGN_IN_LABEL)
                    .into_any_element();
                let recheck = tray_button(element_id(RECHECK_PREFIX, id), false, pal)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.check_runner_logins(runner, cx);
                    }))
                    .child(RECHECK_LABEL)
                    .into_any_element();
                let text = format!(
                    "{} {SIGNED_OUT_MIDDLE} {}",
                    cli.label(),
                    self.runner_name(runner)
                );
                (text, vec![recheck, sign_in])
            }
        };
        Some(
            tray(pal)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_2))
                        .child(icon(IconName::Cloud, ICON, pal.text_muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(rgb(pal.text_default))
                                .child(text),
                        )
                        .children(buttons),
                )
                .into_any_element(),
        )
    }
}
