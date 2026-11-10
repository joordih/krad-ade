use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, IntoElement, ParentElement, SharedString, Styled,
};
use krad_domain::{RunTotals, TestVerdict, Variant, VariantRun, VariantStatus};
use std::time::SystemTime;

use super::super::spending::token_count;
use super::super::EnvironmentShell;
use super::view::duration_text;
use crate::components::{harness_icon, icon, Button, ButtonVariant, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_LG, SPACE_1, SPACE_2, SPACE_3, TEXT_SM,
    TEXT_XS,
};

const SHOWN_FILES: usize = 8;
const NO_DIFF_TEXT: &str = "No diff yet";
const NO_TESTS_TEXT: &str = "No tests chosen";
const RUNNING_FOR: &str = "Running for ";
const TOOK: &str = "Took ";
const IN_SEPARATOR: &str = " in ";
const MORE_FILES_SUFFIX: &str = " more files";
const FILES_SUFFIX: &str = " files";
const TOKENS_SUFFIX: &str = " tokens";
const FACT_SEPARATOR: &str = " · ";
const CHEAPEST_LABEL: &str = "Cheapest";
const FASTEST_LABEL: &str = "Fastest";
const OPEN_LABEL: &str = "Open";
const TEST_LABEL: &str = "Run tests";
const PICK_LABEL: &str = "Pick";
const OPEN_PREFIX: &str = "variant-open";
const TEST_PREFIX: &str = "variant-test";
const PICK_PREFIX: &str = "variant-pick";

fn fact(text: impl Into<SharedString>, color: u32) -> AnyElement {
    div()
        .truncate()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(color))
        .child(text.into())
        .into_any_element()
}

fn status_color(status: &VariantStatus, pal: &Palette) -> u32 {
    match status {
        VariantStatus::Tested {
            verdict: TestVerdict::Passed,
        }
        | VariantStatus::Chosen => pal.success_fg,
        VariantStatus::Tested { .. } | VariantStatus::Failed { .. } | VariantStatus::Missing => {
            pal.error_fg
        }
        VariantStatus::Launching | VariantStatus::Working | VariantStatus::Testing => pal.info_fg,
        VariantStatus::Finished | VariantStatus::Discarded => pal.text_subtle,
    }
}

fn diff_facts(variant: &Variant, pal: &Palette) -> Vec<AnyElement> {
    let Some(diff) = &variant.diff else {
        return vec![fact(NO_DIFF_TEXT, pal.text_muted)];
    };
    let head = format!(
        "{}{FILES_SUFFIX}{FACT_SEPARATOR}+{} −{}",
        diff.file_count(),
        diff.added,
        diff.removed
    );
    let more = diff.file_count().saturating_sub(SHOWN_FILES);
    std::iter::once(fact(head, pal.text_default))
        .chain(diff.files.iter().take(SHOWN_FILES).map(|file| {
            fact(
                format!("+{} −{}  {}", file.added, file.removed, file.path.display()),
                pal.text_subtle,
            )
        }))
        .chain((more > 0).then(|| fact(format!("{more}{MORE_FILES_SUFFIX}"), pal.text_muted)))
        .collect()
}

fn test_fact(run: &VariantRun, variant: &Variant, pal: &Palette) -> AnyElement {
    match (&variant.test, &run.test) {
        (Some(outcome), _) => fact(
            format!(
                "{}{IN_SEPARATOR}{}",
                outcome.verdict.label(),
                duration_text(outcome.took)
            ),
            match outcome.verdict {
                TestVerdict::Passed => pal.success_fg,
                TestVerdict::Failed | TestVerdict::CouldNotRun => pal.error_fg,
            },
        ),
        (None, Some(task)) => fact(task.to_string(), pal.text_muted),
        (None, None) => fact(NO_TESTS_TEXT, pal.text_muted),
    }
}

impl EnvironmentShell {
    pub(super) fn variant_column(
        &self,
        run: &VariantRun,
        variant: &Variant,
        totals: &RunTotals,
        now: SystemTime,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let harness = self.settings.acp.harness_name(variant.spec.harness);
        let (mark, mark_color) = harness_icon(variant.spec.harness, pal);
        let elapsed = variant.elapsed(now).map(|took| {
            let prefix = if variant.finished_at.is_some() {
                TOOK
            } else {
                RUNNING_FOR
            };
            format!("{prefix}{}", duration_text(took))
        });
        let badges: Vec<&str> = [
            (totals.cheapest == Some(variant.id)).then_some(CHEAPEST_LABEL),
            (totals.fastest == Some(variant.id)).then_some(FASTEST_LABEL),
        ]
        .into_iter()
        .flatten()
        .collect();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(SPACE_1))
            .p(px(SPACE_3))
            .rounded(px(RADIUS_LG))
            .bg(rgb(pal.bg_panel))
            .border_1()
            .border_color(pal.divider)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1))
                    .child(icon(mark, ICON_SM, mark_color))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_strong))
                            .child(format!("{} {}", variant.id, variant.spec.label(&harness))),
                    ),
            )
            .child(fact(
                variant.status.label(),
                status_color(&variant.status, pal),
            ))
            .when(!badges.is_empty(), |column| {
                column.child(fact(badges.join(FACT_SEPARATOR), pal.accent))
            })
            .child(fact(
                format!(
                    "{}{FACT_SEPARATOR}{}{TOKENS_SUFFIX}",
                    variant.spent.cost,
                    token_count(variant.spent.tokens.total())
                ),
                pal.text_default,
            ))
            .children(elapsed.map(|elapsed| fact(elapsed, pal.text_subtle)))
            .child(test_fact(run, variant, pal))
            .children(diff_facts(variant, pal))
            .children(
                variant
                    .spec
                    .instruction
                    .as_ref()
                    .map(|text| fact(text.clone(), pal.text_muted)),
            )
            .child(Self::variant_actions(run, variant, cx))
            .into_any_element()
    }

    fn variant_actions(run: &VariantRun, variant: &Variant, cx: &mut Context<Self>) -> AnyElement {
        let id = variant.id;
        let run_id = run.id;
        let decided = run.decision.is_some();
        let can_test = run.test.is_some() && variant.status.can_win() && !decided;
        let open = variant
            .terminal
            .filter(|_| !variant.status.is_closed() || variant.status == VariantStatus::Chosen);
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_2))
            .pt(px(SPACE_2))
            .children(open.map(|terminal| {
                Button::new((OPEN_PREFIX, u64::from(id.0)))
                    .label(OPEN_LABEL)
                    .icon(IconName::OpenExternal)
                    .variant(ButtonVariant::Secondary)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.go_to_variant(terminal, cx);
                    }))
            }))
            .when(can_test, |actions| {
                actions.child(
                    Button::new((TEST_PREFIX, u64::from(id.0)))
                        .label(TEST_LABEL)
                        .icon(IconName::Play)
                        .variant(ButtonVariant::Secondary)
                        .small()
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            let env = this.active.clone();
                            this.retest_variant(&env, run_id, id, cx);
                        })),
                )
            })
            .when(variant.status.can_win() && !decided, |actions| {
                actions.child(
                    Button::new((PICK_PREFIX, u64::from(id.0)))
                        .label(PICK_LABEL)
                        .icon(IconName::Crown)
                        .variant(ButtonVariant::Primary)
                        .small()
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.ask_pick(id, cx);
                        })),
                )
            })
            .into_any_element()
    }
}
