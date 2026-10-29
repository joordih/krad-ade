use gpui_kit::{div, px, rgb, AnyElement, Context, IntoElement, ParentElement, Styled, Window};

use super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title};
use super::native_alert::Confirmation;
use super::EnvironmentShell;
use crate::actions::{CloseWindow, Quit};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_5, SPACE_6, TEXT_SM};

const CLOSE_DIALOG_TITLE: &str = "Quit Krad ADE?";
const CLOSE_DIALOG_CANCEL: &str = "Cancel";
const CLOSE_DIALOG_CONFIRM: &str = "Quit and end sessions";
const BTN_CLOSE_CANCEL: &str = "close-cancel";
const BTN_CLOSE_CONFIRM: &str = "close-confirm";
const BTN_CLOSE_DISMISS: &str = "close-dismiss";
const CLOSE_DIALOG_W: f32 = 400.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CloseThen {
    RemoveWindow,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CloseRequest {
    Idle,
    Confirming(CloseThen),
    Asking,
    Confirmed,
}

fn cancel(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.cancel_close(cx);
}

fn close_dialog_text(running: usize) -> String {
    format!("{running} terminals are still running. Closing the window ends their processes.")
}

impl EnvironmentShell {
    pub(super) fn any_alive_count(&self) -> usize {
        self.envs
            .values()
            .flat_map(|live| live.sessions.values())
            .filter(|session| session.state.is_running())
            .count()
    }

    pub fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.may_close(CloseThen::RemoveWindow, window, cx)
    }

    fn may_close(&mut self, then: CloseThen, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match self.close_request {
            CloseRequest::Asking => return false,
            CloseRequest::Idle | CloseRequest::Confirming(_) if self.any_alive_count() > 0 => {
                self.ask_to_close(then, window, cx);
                return false;
            }
            CloseRequest::Idle | CloseRequest::Confirming(_) | CloseRequest::Confirmed => {}
        }
        self.store();
        true
    }

    fn ask_to_close(&mut self, then: CloseThen, window: &mut Window, cx: &mut Context<Self>) {
        let detail = close_dialog_text(self.any_alive_count());
        let confirmation = Confirmation {
            title: CLOSE_DIALOG_TITLE,
            detail: &detail,
            confirm: CLOSE_DIALOG_CONFIRM,
            cancel: CLOSE_DIALOG_CANCEL,
        };
        let asked = Self::confirm_natively(
            &confirmation,
            window,
            cx,
            move |this, confirmed, window, cx| {
                if confirmed {
                    this.confirm_close(then, window, cx);
                } else {
                    this.cancel_close(cx);
                }
            },
        );
        self.close_request = if asked {
            CloseRequest::Asking
        } else {
            CloseRequest::Confirming(then)
        };
        cx.notify();
    }

    pub(super) fn on_close_window(
        &mut self,
        _: &CloseWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.may_close(CloseThen::RemoveWindow, window, cx) {
            self.confirm_close(CloseThen::RemoveWindow, window, cx);
        }
    }

    pub(super) fn on_quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.may_close(CloseThen::Quit, window, cx) {
            self.confirm_close(CloseThen::Quit, window, cx);
        }
    }

    pub(super) fn confirm_close(
        &mut self,
        then: CloseThen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_request = CloseRequest::Confirmed;
        self.store();
        window.remove_window();
        if then == CloseThen::Quit {
            cx.quit();
        }
    }

    pub(super) fn cancel_close(&mut self, cx: &mut Context<Self>) {
        self.close_request = CloseRequest::Idle;
        cx.notify();
    }

    pub(super) fn close_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let CloseRequest::Confirming(then) = self.close_request else {
            return div().into_any_element();
        };
        let running = self.any_alive_count();
        let sheet = sheet_panel(CLOSE_DIALOG_W, pal)
            .child(sheet_title(
                CLOSE_DIALOG_TITLE,
                None,
                BTN_CLOSE_DISMISS,
                cancel,
                cx,
            ))
            .child(
                div()
                    .px(px(SPACE_6))
                    .pb(px(SPACE_5))
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_muted))
                    .child(close_dialog_text(running)),
            )
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_CLOSE_CANCEL)
                            .label(CLOSE_DIALOG_CANCEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.cancel_close(cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_CLOSE_CONFIRM)
                            .label(CLOSE_DIALOG_CONFIRM)
                            .variant(ButtonVariant::Destructive)
                            .on_click(cx.listener(
                                move |this: &mut Self, _, window: &mut Window, cx| {
                                    this.confirm_close(then, window, cx);
                                },
                            )),
                    ),
            );
        sheet_overlay(sheet, cancel, cx)
    }
}
