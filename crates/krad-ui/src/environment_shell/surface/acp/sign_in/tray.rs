use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{AcpSignIn, TerminalId};

use super::super::super::super::ask::{tray, tray_button};
use super::super::super::super::EnvironmentShell;
use super::super::super::SurfaceChat;
use super::{SignInAsk, SignInStep};
use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{palette, ICON, LINE_XS, SPACE_1, SPACE_2, TEXT_XS};

const METHOD_PREFIX: &str = "acp-sign-in-method";
const TERMINAL_PREFIX: &str = "acp-sign-in-terminal";
const TITLE_TEXT: &str = "Sign in to the agent to start this chat";
const SIGNING_TEXT: &str = "Signing in…";
const FAILED_TEXT: &str = "Sign-in did not finish. Choose a way to sign in again.";
const IN_TERMINAL_TEXT: &str = "Finish signing in in the terminal; the chat goes on once it ends.";
const TERMINAL_LABEL: &str = "Sign in in a terminal";
const TERMINAL_METHOD_TOOLTIP: &str = "Opens a terminal to sign in";

fn element_id(prefix: &str, id: TerminalId, index: usize) -> ElementId {
    ElementId::Name(format!("{prefix}-{}-{index}", id.as_u64()).into())
}

const fn step_text(step: &SignInStep) -> &'static str {
    match step {
        SignInStep::Choosing => TITLE_TEXT,
        SignInStep::Signing(_) => SIGNING_TEXT,
        SignInStep::Failed(_) => FAILED_TEXT,
        SignInStep::InTerminal { .. } => IN_TERMINAL_TEXT,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn acp_sign_in_tray(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let asked: &SignInAsk = chat.structured()?.acp.sign_in.as_ref()?;
        let pal = palette(cx);
        let waits = matches!(
            asked.step,
            SignInStep::Signing(_) | SignInStep::InTerminal { .. }
        );
        let failed = matches!(asked.step, SignInStep::Failed(_))
            && chat
                .harness()
                .acp_agent()
                .and_then(|agent| self.settings.acp.agent(agent))
                .is_some_and(|agent| agent.terminal.is_some());
        let methods = asked.methods.iter().enumerate().map(|(index, method)| {
            let picked = method.id.clone();
            let in_terminal = matches!(method.sign_in, AcpSignIn::InTerminal { .. });
            tray_button(element_id(METHOD_PREFIX, id, index), index == 0, pal)
                .when(in_terminal, |button| {
                    button.tooltip(|_, cx| tooltip_view(TERMINAL_METHOD_TOOLTIP.into(), cx))
                })
                .when(!waits, |button| {
                    button.on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.pick_sign_in(id, &picked, cx);
                    }))
                })
                .child(method.name.clone())
                .into_any_element()
        });
        let descriptions = asked.methods.iter().filter_map(|method| {
            method.description.as_ref().map(|description| {
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(format!("{}: {description}", method.name))
            })
        });
        Some(
            tray(pal)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_2))
                        .child(icon(IconName::Lock, ICON, pal.text_muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(rgb(pal.text_default))
                                .child(step_text(&asked.step)),
                        ),
                )
                .children(descriptions)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(SPACE_1))
                        .children(methods)
                        .when(failed, |row| {
                            row.child(
                                tray_button(element_id(TERMINAL_PREFIX, id, 0), false, pal)
                                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                        this.sign_in_in_terminal(id, cx);
                                    }))
                                    .child(TERMINAL_LABEL),
                            )
                        }),
                )
                .into_any_element(),
        )
    }
}
