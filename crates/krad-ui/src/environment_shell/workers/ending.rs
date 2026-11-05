use gpui_kit::{div, px, rgb, AnyElement, Context, ParentElement, Styled, Window};
use krad_domain::{AgentState, SessionId, TerminalId};

use super::super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title};
use super::super::native_alert::Confirmation;
use super::super::EnvironmentShell;
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_5, SPACE_6, TEXT_SM};

const STOP_TITLE: &str = "Stop its workers?";
const STOP_TEXT: &str = "Workers of this lead are still working. They stop and their sessions close; their chats can be reopened later from the lead.";
const STOP_CONFIRM: &str = "Stop workers";
const CANCEL: &str = "Cancel";
const BTN_CLOSE: &str = "lead-ending-close";
const BTN_CANCEL: &str = "lead-ending-cancel";
const BTN_CONFIRM: &str = "lead-ending-confirm";
const DIALOG_W: f32 = 420.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum LeadEnding {
    Dismiss(SessionId),
    CloseSession(SessionId),
}

impl LeadEnding {
    const fn session(self) -> SessionId {
        match self {
            Self::Dismiss(session) | Self::CloseSession(session) => session,
        }
    }
}

fn cancel(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    if let Some(live) = this.envs.get_mut(&this.active) {
        live.pending_lead_ending = None;
    }
    cx.notify();
}

impl EnvironmentShell {
    fn open_workers_of(&self, session: SessionId) -> Vec<TerminalId> {
        self.envs
            .get(&self.active)
            .map_or_else(Vec::new, |live| live.data.open_workers(session))
    }

    fn has_working_workers(&self, session: SessionId) -> bool {
        let Some(live) = self.envs.get(&self.active) else {
            return false;
        };
        live.data.open_workers(session).iter().any(|terminal| {
            live.data
                .nodes
                .iter()
                .find(|node| node.id == *terminal)
                .is_some_and(|node| {
                    node.state != AgentState::Idle && node.state != AgentState::Done
                })
        })
    }

    pub(in crate::environment_shell) fn ask_end_lead(
        &mut self,
        ending: LeadEnding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_working_workers(ending.session()) {
            self.end_lead(ending, cx);
            return;
        }
        let confirmation = Confirmation {
            title: STOP_TITLE,
            detail: STOP_TEXT,
            confirm: STOP_CONFIRM,
            cancel: CANCEL,
        };
        let asked =
            Self::confirm_natively(&confirmation, window, cx, move |this, confirmed, _, cx| {
                if confirmed {
                    this.end_lead(ending, cx);
                }
            });
        if !asked {
            if let Some(live) = self.envs.get_mut(&self.active) {
                live.pending_lead_ending = Some(ending);
            }
            cx.notify();
        }
    }

    fn end_lead(&mut self, ending: LeadEnding, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let session = ending.session();
        for terminal in self.open_workers_of(session) {
            self.close_terminal_in(&env, terminal, cx);
        }
        match ending {
            LeadEnding::Dismiss(session) => self.dismiss_lead(session, cx),
            LeadEnding::CloseSession(session) => self.close_session(session, cx),
        }
    }

    fn confirm_lead_ending(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let pending = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.pending_lead_ending.take());
        if let Some(ending) = pending {
            self.end_lead(ending, cx);
        }
    }

    pub(in crate::environment_shell) fn lead_ending_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.envs.get(&self.active)?.pending_lead_ending?;
        let pal = palette(cx);
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(STOP_TITLE, None, BTN_CLOSE, cancel, cx))
            .child(
                div()
                    .px(px(SPACE_6))
                    .pb(px(SPACE_5))
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_muted))
                    .child(STOP_TEXT),
            )
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_CANCEL)
                            .label(CANCEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                cancel(this, window, cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_CONFIRM)
                            .label(STOP_CONFIRM)
                            .variant(ButtonVariant::Destructive)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.confirm_lead_ending(cx);
                            })),
                    ),
            );
        Some(sheet_overlay(sheet, cancel, cx))
    }
}
