use gpui_kit::{
    div, px, rgb, AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, Window,
};
use krad_domain::KraddieId;

use super::super::native_alert::Confirmation;
use super::super::EnvironmentShell;
use crate::components::{Button, ButtonVariant};
use crate::theme::{Palette, LINE_XS, SPACE_1, SPACE_2, TEXT_XS};

const FAREWELL_TITLE: &str = "Delete this Kraddie?";
const FAREWELL_WHAT: &str = "Its automations and their run history, its keepsakes, its chat digests and its profile are deleted for good. Chats it already had stay open.";
const FAREWELL_CONFIRM: &str = "Delete";
const FAREWELL_CANCEL: &str = "Keep it";
const FAREWELL_SEPARATOR: &str = "\n\n";
const KEEP_PREFIX: &str = "kraddie-farewell-keep";
const DELETE_PREFIX: &str = "kraddie-farewell-delete";

fn farewell_detail(name: &str) -> String {
    format!("{name}{FAREWELL_SEPARATOR}{FAREWELL_WHAT}")
}

impl EnvironmentShell {
    pub(super) fn ask_delete_kraddie(
        &mut self,
        id: &KraddieId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.kraddies.entry(id) else {
            return;
        };
        let detail = farewell_detail(entry.kraddie.name.as_str());
        let confirmation = Confirmation {
            title: FAREWELL_TITLE,
            detail: &detail,
            confirm: FAREWELL_CONFIRM,
            cancel: FAREWELL_CANCEL,
        };
        let target = id.clone();
        let asked =
            Self::confirm_natively(&confirmation, window, cx, move |this, confirmed, _, cx| {
                if confirmed {
                    this.delete_kraddie(&target, cx);
                }
            });
        if !asked {
            self.kraddies.deleting = Some(id.clone());
            cx.notify();
        }
    }

    pub(super) fn farewell_row(
        &self,
        id: &KraddieId,
        key: &SharedString,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.kraddies.deleting.as_ref() != Some(id) {
            return None;
        }
        let target = id.clone();
        let buttons = div()
            .flex()
            .flex_row()
            .justify_end()
            .gap(px(SPACE_2))
            .child(
                Button::new(SharedString::from(format!("{KEEP_PREFIX}-{key}")))
                    .label(FAREWELL_CANCEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.kraddies.deleting = None;
                        cx.notify();
                    })),
            )
            .child(
                Button::new(SharedString::from(format!("{DELETE_PREFIX}-{key}")))
                    .label(FAREWELL_CONFIRM)
                    .small()
                    .variant(ButtonVariant::Destructive)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.kraddies.deleting = None;
                        this.delete_kraddie(&target, cx);
                    })),
            );
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .child(
                    div()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(FAREWELL_WHAT),
                )
                .child(buttons)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_farewell_names_the_kraddie_and_what_goes_with_it() {
        let detail = farewell_detail("Night Owl");
        assert!(detail.starts_with("Night Owl\n\n"));
        for part in [
            "automations",
            "run history",
            "keepsakes",
            "chat digests",
            "profile",
        ] {
            assert!(detail.contains(part), "{part}");
        }
    }
}
