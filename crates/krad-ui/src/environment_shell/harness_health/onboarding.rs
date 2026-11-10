use gpui_kit::{div, px, rgb, AnyElement, Context, ParentElement, Styled, Window};

use super::super::dialog_parts::{
    sheet_body, sheet_footer, sheet_group, sheet_overlay, sheet_panel, sheet_title,
};
use super::super::EnvironmentShell;
use super::{health_rows, HealthPlace};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_2, SPACE_5, SPACE_6, TEXT_SM};

const SHEET_W: f32 = 560.0;
const TITLE: &str = "Set up a coding agent";
const SUBTITLE: &str = "None of the agents the app runs is ready on this computer yet.";
const INTRO: &str = "Krad ADE runs Claude Code, Codex and opencode in chats and terminals. Fix one of them below, then check again. The same checks stay in Settings, under Harnesses.";
const CHECK_AGAIN: &str = "Check again";
const LATER: &str = "Not now";
const BTN_CLOSE: &str = "harness-onboarding-close";
const BTN_CHECK: &str = "harness-onboarding-check";
const BTN_LATER: &str = "harness-onboarding-later";
const BODY_ID: &str = "harness-onboarding-body";
const COPY_PREFIX: &str = "harness-onboarding-copy";

fn dismiss(shell: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    shell.finish_harness_onboarding(cx);
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn harness_onboarding_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.harness_checks.onboarding_open {
            return None;
        }
        let rows = health_rows(self, &HealthPlace::ThisComputer, COPY_PREFIX, cx);
        let pal = palette(cx);
        let intro = div()
            .px(px(SPACE_6))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_muted))
            .child(INTRO);
        let body = sheet_body(BODY_ID)
            .gap(px(SPACE_5))
            .pb(px(SPACE_5))
            .child(intro)
            .child(div().px(px(SPACE_6)).child(sheet_group(rows, pal)));
        let footer = sheet_footer(pal)
            .justify_end()
            .gap(px(SPACE_2))
            .child(
                Button::new(BTN_LATER)
                    .label(LATER)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.finish_harness_onboarding(cx);
                    })),
            )
            .child(
                Button::new(BTN_CHECK)
                    .label(CHECK_AGAIN)
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.recheck_onboarding(cx);
                    })),
            );
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(
                TITLE,
                Some(SUBTITLE.to_owned()),
                BTN_CLOSE,
                dismiss,
                cx,
            ))
            .child(body)
            .child(footer);
        Some(sheet_overlay(sheet, dismiss, cx))
    }
}
