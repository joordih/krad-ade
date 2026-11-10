use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, Window,
};
use krad_domain::{
    GroupId, RunTotals, TerminalId, VariantId, VariantRun, VariantRunId, WinnerOutcome,
};
use krad_repo::VariantRemoval;
use std::time::SystemTime;

use super::super::dialog_parts::{
    segment, sheet_body, sheet_footer, sheet_overlay, sheet_panel, sheet_title, SegmentSize,
};
use super::super::EnvironmentShell;
use super::pick::VariantPick;
use super::NO_GROUP_TEXT;
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, LINE_SM, SPACE_1, SPACE_2, TEXT_SM};

const SHEET_W: f32 = 1080.0;
const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_SUFFIX: &str = "s";
const MINUTES_SUFFIX: &str = "m";
const TITLE: &str = "Variants";
const NO_RUNS_TEXT: &str = "No variant runs in this group yet";
const RUN_PREFIX: &str = "Run ";
const TOTAL_PREFIX: &str = "Total ";
const WALL_PREFIX: &str = " · wall time ";
const FACT_SEPARATOR: &str = " · ";
const TOKENS_SUFFIX: &str = " tokens";
const PICK_PREFIX: &str = "Pick ";
const DISCARD_SUFFIX: &str =
    " as the winner? The other variants are discarded and their worktrees and branches removed.";
const CONFIRM_LABEL: &str = "Pick the winner";
const CANCEL_LABEL: &str = "Cancel";
const NEW_RUN_LABEL: &str = "New run…";
const BTN_CLOSE: &str = "variants-view-close";
const BTN_CONFIRM: &str = "variants-confirm";
const BTN_CANCEL: &str = "variants-cancel";
const BTN_NEW_RUN: &str = "variants-new-run";
const BODY_ID: &str = "variants-view-body";
const RUN_SEGMENT_PREFIX: &str = "variants-run";
const OUTCOME_PREFIX: &str = "variants-outcome";
const OUTCOMES: [WinnerOutcome; 2] = [WinnerOutcome::KeepWorktree, WinnerOutcome::ApplyOntoBase];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PickConfirm {
    pub(super) variant: VariantId,
    pub(super) outcome: WinnerOutcome,
}

pub(super) struct VariantsView {
    pub(super) group: GroupId,
    pub(super) run: Option<VariantRunId>,
    pub(super) confirm: Option<PickConfirm>,
}

fn close(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_variants_ui(cx);
}

pub(super) fn duration_text(took: std::time::Duration) -> String {
    let seconds = took.as_secs();
    let (minutes, seconds) = (seconds / SECONDS_PER_MINUTE, seconds % SECONDS_PER_MINUTE);
    if minutes == 0 {
        return format!("{seconds}{SECONDS_SUFFIX}");
    }
    format!("{minutes}{MINUTES_SUFFIX} {seconds:02}{SECONDS_SUFFIX}")
}

fn totals_text(totals: &RunTotals) -> String {
    let wall = totals
        .wall
        .map_or_default(|wall| format!("{WALL_PREFIX}{}", duration_text(wall)));
    format!(
        "{TOTAL_PREFIX}{}{FACT_SEPARATOR}{}{TOKENS_SUFFIX}{wall}",
        totals.spent.cost,
        super::super::spending::token_count(totals.spent.tokens.total())
    )
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_variants_view(&mut self, cx: &mut Context<Self>) {
        let Some(group) = self.active_group().map(|group| group.id) else {
            return self.toast(NO_GROUP_TEXT, cx);
        };
        let base = self.envs.get(&self.active).and_then(|live| {
            live.data.variant_runs.all().iter().find_map(|run| {
                run.variants
                    .iter()
                    .any(|variant| {
                        variant
                            .worktree
                            .as_ref()
                            .is_some_and(|tree| tree.group == group)
                    })
                    .then_some((run.group, run.id))
            })
        });
        match base {
            Some((base, run)) => self.show_variant_run(base, Some(run), cx),
            None => self.show_variant_run(group, None, cx),
        }
    }

    pub(super) fn show_variant_run(
        &mut self,
        group: GroupId,
        run: Option<VariantRunId>,
        cx: &mut Context<Self>,
    ) {
        self.variants.draft = None;
        self.variants.view = Some(VariantsView {
            group,
            run,
            confirm: None,
        });
        let env = self.active.clone();
        self.check_variant_worktrees(&env, cx);
        cx.notify();
    }

    fn shown_run(&self) -> Option<&VariantRun> {
        let view = self.variants.view.as_ref()?;
        let runs = &self.envs.get(&self.active)?.data.variant_runs;
        view.run
            .and_then(|run| runs.run(run))
            .or_else(|| runs.latest(view.group))
    }

    pub(super) fn ask_pick(&mut self, variant: VariantId, cx: &mut Context<Self>) {
        if let Some(view) = &mut self.variants.view {
            view.confirm = Some(PickConfirm {
                variant,
                outcome: WinnerOutcome::KeepWorktree,
            });
        }
        cx.notify();
    }

    pub(super) fn go_to_variant(&mut self, terminal: TerminalId, cx: &mut Context<Self>) {
        self.close_variants_ui(cx);
        self.focus_terminal(terminal);
        cx.notify();
    }

    fn confirm_pick(&mut self, cx: &mut Context<Self>) {
        let Some((run, confirm)) = self
            .shown_run()
            .map(|run| run.id)
            .zip(self.variants.view.as_ref().and_then(|view| view.confirm))
        else {
            return;
        };
        if let Some(view) = &mut self.variants.view {
            view.confirm = None;
        }
        let env = self.active.clone();
        let pick = VariantPick {
            run,
            winner: confirm.variant,
            outcome: confirm.outcome,
            removal: VariantRemoval::Forced,
        };
        self.pick_variant(&env, pick, None, cx);
    }

    pub(in crate::environment_shell) fn variants_view_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = self.variants.view.as_ref()?;
        let pal = palette(cx);
        let now = SystemTime::now();
        let run = self.shown_run();
        let subtitle = run.map(|run| {
            format!(
                "{RUN_PREFIX}{}{FACT_SEPARATOR}{}{FACT_SEPARATOR}{}",
                run.id,
                run.phase().label(),
                run.prompt.lines().next().unwrap_or_default()
            )
        });
        let totals = run.map_or_default(|run| run.totals(now));
        let body = match run {
            Some(run) => sheet_body(BODY_ID)
                .child(self.run_tabs(view, run.id, cx))
                .child(
                    div().flex().flex_row().gap(px(SPACE_2)).children(
                        run.variants
                            .iter()
                            .map(|variant| self.variant_column(run, variant, &totals, now, cx)),
                    ),
                ),
            None => sheet_body(BODY_ID).child(
                div()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_subtle))
                    .child(NO_RUNS_TEXT),
            ),
        };
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(TITLE, subtitle, BTN_CLOSE, close, cx))
            .child(body)
            .child(self.view_footer(view, run, &totals, cx));
        Some(sheet_overlay(sheet, close, cx))
    }

    fn run_tabs(
        &self,
        view: &VariantsView,
        shown: VariantRunId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let runs: Vec<VariantRunId> = self.envs.get(&self.active).map_or_default(|live| {
            live.data
                .variant_runs
                .of_group(view.group)
                .map(|run| run.id)
                .collect()
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .when(runs.len() > 1, |tabs| {
                tabs.children(runs.into_iter().rev().map(|run| {
                    segment(
                        (RUN_SEGMENT_PREFIX, run.0),
                        format!("{RUN_PREFIX}{run}"),
                        None,
                        run == shown,
                        SegmentSize::Compact,
                        pal,
                        cx,
                    )
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        if let Some(view) = &mut this.variants.view {
                            view.run = Some(run);
                            view.confirm = None;
                        }
                        cx.notify();
                    }))
                    .into_any_element()
                }))
            })
            .into_any_element()
    }

    fn view_footer(
        &self,
        view: &VariantsView,
        run: Option<&VariantRun>,
        totals: &RunTotals,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let footer = sheet_footer(pal);
        if let Some((run, confirm)) = run.zip(view.confirm) {
            let question = format!(
                "{PICK_PREFIX}{}{DISCARD_SUFFIX}",
                self.variant_label(run, confirm.variant)
            );
            let outcomes = OUTCOMES.iter().enumerate().map(|(index, outcome)| {
                let outcome = *outcome;
                segment(
                    (OUTCOME_PREFIX, index),
                    outcome.label(),
                    None,
                    confirm.outcome == outcome,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    if let Some(confirm) = this
                        .variants
                        .view
                        .as_mut()
                        .and_then(|view| view.confirm.as_mut())
                    {
                        confirm.outcome = outcome;
                    }
                    cx.notify();
                }))
                .into_any_element()
            });
            return div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(SPACE_2))
                        .pt(px(SPACE_2))
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_1))
                        .text_size(px(TEXT_SM))
                        .text_color(rgb(pal.text_default))
                        .child(question)
                        .child(div().flex().flex_row().gap(px(SPACE_1)).children(outcomes)),
                )
                .child(
                    footer
                        .child(div().flex_1())
                        .child(
                            Button::new(BTN_CANCEL)
                                .label(CANCEL_LABEL)
                                .variant(ButtonVariant::Secondary)
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    if let Some(view) = &mut this.variants.view {
                                        view.confirm = None;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(BTN_CONFIRM)
                                .label(CONFIRM_LABEL)
                                .icon(IconName::Crown)
                                .variant(ButtonVariant::Destructive)
                                .on_click(
                                    cx.listener(|this: &mut Self, _, _, cx| this.confirm_pick(cx)),
                                ),
                        ),
                )
                .into_any_element();
        }
        footer
            .children(run.map(|_| {
                div()
                    .text_size(px(TEXT_SM))
                    .text_color(rgb(pal.text_subtle))
                    .child(totals_text(totals))
            }))
            .child(div().flex_1())
            .child(
                Button::new(BTN_NEW_RUN)
                    .label(NEW_RUN_LABEL)
                    .icon(IconName::Workflow)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.open_variants_draft(window, cx);
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn durations_read_in_seconds_then_minutes() {
        assert_eq!(duration_text(Duration::from_secs(42)), "42s");
        assert_eq!(duration_text(Duration::from_secs(185)), "3m 05s");
    }
}
