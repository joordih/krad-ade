use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement};
use krad_domain::{UsageIndicator, WarnPercent};

use super::super::dialog_parts::{segment, SegmentSize};
use super::spending::WARNINGS;
use super::{form_row, panel_card, segmented_track, EnvironmentShell};
use crate::theme::palette;

const INDICATOR_PREFIX: &str = "plan-usage-indicator";
const WARNING_PREFIX: &str = "plan-usage-warning";
const INDICATOR_LABEL: &str = "Usage bar";
const INDICATOR_HINT: &str = "A strip at the bottom of the window with how much of each harness's subscription is used in each of its windows and when they reset; hover a harness for its plan and every window.";
const WARNING_LABEL: &str = "Warn at";
const WARNING_HINT: &str = "A window turns to the warning colour once it reaches this share.";
const INDICATORS: [(&str, UsageIndicator); 2] = [
    ("Shown", UsageIndicator::Shown),
    ("Hidden", UsageIndicator::Hidden),
];

impl EnvironmentShell {
    fn set_usage_indicator(&mut self, indicator: UsageIndicator, cx: &mut Context<Self>) {
        self.settings.plan_usage.indicator = indicator;
        self.save_settings(cx);
        cx.notify();
    }

    fn set_plan_warning(&mut self, warning: WarnPercent, cx: &mut Context<Self>) {
        self.settings.plan_usage.warn_at = warning;
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn plan_usage_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.plan_usage;
        let indicators = INDICATORS.iter().map(|&(label, indicator)| {
            segment(
                (INDICATOR_PREFIX, indicator as u64),
                label,
                None,
                current.indicator == indicator,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_usage_indicator(indicator, cx);
            }))
        });
        let indicator_row = form_row(
            INDICATOR_LABEL,
            Some(INDICATOR_HINT),
            segmented_track(pal).children(indicators).into_any_element(),
            pal,
        );
        let warnings = WARNINGS.iter().filter_map(|&(label, percent)| {
            let warning = WarnPercent::try_from(percent).ok()?;
            Some(
                segment(
                    (WARNING_PREFIX, u64::from(percent)),
                    label,
                    None,
                    current.warn_at == warning,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.set_plan_warning(warning, cx);
                })),
            )
        });
        let warning_row = (current.indicator == UsageIndicator::Shown).then(|| {
            form_row(
                WARNING_LABEL,
                Some(WARNING_HINT),
                segmented_track(pal).children(warnings).into_any_element(),
                pal,
            )
        });
        panel_card(std::iter::once(indicator_row).chain(warning_row), pal)
    }
}
