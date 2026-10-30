use gpui_kit::{
    div, px, AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::{AppUpdateCheck, AppearanceMode};

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell, SWATCH_GAP};
use crate::components::IconName;
use crate::theme::palette;

const BTN_LIGHT: &str = "appearance-light";
const BTN_DARK: &str = "appearance-dark";
const LABEL_LIGHT: &str = "Light";
const LABEL_DARK: &str = "Dark";
const APPEARANCE_LABEL: &str = "Appearance";
const APPEARANCE_HINT: &str =
    "Light or dark, for the whole app, terminals and code editor included.";
const BTN_UPDATES_AUTOMATIC: &str = "app-updates-automatic";
const BTN_UPDATES_MANUAL: &str = "app-updates-manual";
const LABEL_UPDATES_AUTOMATIC: &str = "Automatic";
const LABEL_UPDATES_MANUAL: &str = "Manual";
const UPDATES_LABEL: &str = "Check for updates";
const UPDATES_HINT: &str = "Automatic downloads new releases in the background and asks you to restart; Manual only checks from the command palette.";

impl EnvironmentShell {
    pub(super) fn general_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.appearance;
        let option =
            |id: &'static str, label: &'static str, glyph: IconName, mode: AppearanceMode| {
                segment(
                    id,
                    label,
                    Some(glyph),
                    current == mode,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.set_appearance(mode, cx);
                }))
            };
        let segmented = segmented_track(pal)
            .child(option(
                BTN_LIGHT,
                LABEL_LIGHT,
                IconName::Sun,
                AppearanceMode::Light,
            ))
            .child(option(
                BTN_DARK,
                LABEL_DARK,
                IconName::Moon,
                AppearanceMode::Dark,
            ))
            .into_any_element();
        let mut appearance_rows = vec![
            form_row(APPEARANCE_LABEL, Some(APPEARANCE_HINT), segmented, pal),
            self.material_row(cx),
        ];
        appearance_rows.extend(self.tint_rows(cx));
        let appearance_card = panel_card(appearance_rows, pal);
        let updates_card = panel_card(vec![self.app_updates_row(cx)], pal);
        div()
            .flex()
            .flex_col()
            .gap(px(SWATCH_GAP))
            .child(appearance_card)
            .child(updates_card)
            .into_any_element()
    }

    fn app_updates_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.app_updates;
        let option = |id: &'static str, label: &'static str, check: AppUpdateCheck| {
            segment(
                id,
                label,
                None,
                current == check,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_app_update_check(check, cx);
            }))
        };
        let segmented = segmented_track(pal)
            .child(option(
                BTN_UPDATES_AUTOMATIC,
                LABEL_UPDATES_AUTOMATIC,
                AppUpdateCheck::Automatic,
            ))
            .child(option(
                BTN_UPDATES_MANUAL,
                LABEL_UPDATES_MANUAL,
                AppUpdateCheck::Manual,
            ))
            .into_any_element();
        form_row(UPDATES_LABEL, Some(UPDATES_HINT), segmented, pal)
    }
}
