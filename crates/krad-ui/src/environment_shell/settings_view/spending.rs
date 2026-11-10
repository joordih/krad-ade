use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement};
use krad_domain::{UsdCost, WarnPercent};

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell};
use crate::theme::palette;

const BUDGET_PREFIX: &str = "group-budget";
const WARNING_PREFIX: &str = "budget-warning";
const BUDGET_LABEL: &str = "Budget per group";
const BUDGET_HINT: &str = "What the agents of one group may spend, in dollars, as their harnesses report it or as the model's price says. Its total shows beside the group in the sidebar.";
const WARNING_LABEL: &str = "Warn at";
const WARNING_HINT: &str =
    "A notice comes once when a group's spend reaches this share of its budget, and once when it goes over.";
const BUDGETS: [(&str, Option<f64>); 6] = [
    ("Off", None),
    ("$5", Some(5.0)),
    ("$20", Some(20.0)),
    ("$50", Some(50.0)),
    ("$100", Some(100.0)),
    ("$250", Some(250.0)),
];
pub(super) const WARNINGS: [(&str, u8); 4] = [("50%", 50), ("75%", 75), ("80%", 80), ("90%", 90)];

fn budget_of(dollars: Option<f64>) -> Option<UsdCost> {
    dollars.and_then(|dollars| UsdCost::try_from(dollars).ok())
}

impl EnvironmentShell {
    fn set_group_budget(&mut self, budget: Option<UsdCost>, cx: &mut Context<Self>) {
        self.settings.spending.group_budget = budget;
        self.save_settings(cx);
        cx.notify();
    }

    fn set_budget_warning(&mut self, warning: WarnPercent, cx: &mut Context<Self>) {
        self.settings.spending.warn_at = warning;
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn spending_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.spending.group_budget;
        let budgets = BUDGETS.iter().enumerate().map(|(at, &(label, dollars))| {
            let budget = budget_of(dollars);
            segment(
                (BUDGET_PREFIX, at as u64),
                label,
                None,
                current == budget,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_group_budget(budget, cx);
            }))
        });
        let budget_row = form_row(
            BUDGET_LABEL,
            Some(BUDGET_HINT),
            segmented_track(pal).children(budgets).into_any_element(),
            pal,
        );
        let warn_at = self.settings.spending.warn_at;
        let warnings = WARNINGS.iter().filter_map(|&(label, percent)| {
            let warning = WarnPercent::try_from(percent).ok()?;
            Some(
                segment(
                    (WARNING_PREFIX, u64::from(percent)),
                    label,
                    None,
                    warn_at == warning,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.set_budget_warning(warning, cx);
                })),
            )
        });
        let warning_row = current.is_some().then(|| {
            form_row(
                WARNING_LABEL,
                Some(WARNING_HINT),
                segmented_track(pal).children(warnings).into_any_element(),
                pal,
            )
        });
        panel_card(std::iter::once(budget_row).chain(warning_row), pal)
    }
}
