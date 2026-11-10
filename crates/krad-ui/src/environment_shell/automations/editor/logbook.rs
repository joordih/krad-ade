use gpui_kit::{
    div, px, rgb, AnyElement, ElementId, FontWeight, IntoElement, ParentElement, SharedString,
    Styled,
};
use std::time::SystemTime;

use super::super::super::EnvironmentShell;
use super::super::card::tone_color;
use super::super::wording::{elapsed, tone};
use super::parts::panel;
use super::AutomationEditor;
use crate::components::motion::rise_in;
use crate::theme::{Palette, LINE_XS, SPACE_1, SPACE_2, TEXT_SM, TEXT_XS};

const LOGBOOK_ROWS: usize = 6;
const LOG_PREFIX: &str = "automation-log";
const DOT: f32 = 6.0;
const LOGBOOK_TITLE: &str = "Logbook";
const LOGBOOK_EMPTY: &str = "Nothing logged yet. Its first run will show up here.";
const WOKE_BY: &str = "woken by";
const LOG_SEPARATOR: &str = " · ";

impl EnvironmentShell {
    pub(super) fn automation_logbook(
        &self,
        editor: &AutomationEditor,
        pal: &Palette,
    ) -> Option<AnyElement> {
        let key = editor.editing.as_ref()?;
        let (_, entry) = self.kraddies.automation(key)?;
        let now = SystemTime::now();
        let rows: Vec<AnyElement> = entry
            .history
            .newest_first()
            .take(LOGBOOK_ROWS)
            .enumerate()
            .map(|(index, run)| {
                let age = now.duration_since(run.started).map_or_default(elapsed);
                rise_in(
                    ElementId::NamedInteger(LOG_PREFIX.into(), run.id.0),
                    index,
                    div()
                        .relative()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_2))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .child(
                            div()
                                .flex_none()
                                .size(px(DOT))
                                .rounded_full()
                                .bg(rgb(tone_color(tone(Some(&run.outcome)), pal))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(pal.text_default))
                                .child(SharedString::from(format!(
                                    "{}{LOG_SEPARATOR}{WOKE_BY} {}",
                                    run.outcome.label(),
                                    run.wake.label()
                                ))),
                        )
                        .child(div().flex_none().text_color(rgb(pal.text_muted)).child(age)),
                )
            })
            .collect();
        let empty = rows.is_empty();
        Some(
            panel(pal)
                .gap(px(SPACE_1))
                .child(
                    div()
                        .text_size(px(TEXT_SM))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(pal.text_strong))
                        .child(LOGBOOK_TITLE),
                )
                .children(rows)
                .children(empty.then(|| {
                    div()
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(LOGBOOK_EMPTY)
                }))
                .into_any_element(),
        )
    }
}
