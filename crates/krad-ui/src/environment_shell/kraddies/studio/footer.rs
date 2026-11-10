use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{KraddieMood, PaintTarget};

use super::super::super::dialog_parts::sheet_footer;
use super::super::super::EnvironmentShell;
use super::KraddieStudio;
use crate::components::{Button, ButtonVariant, IconName};
use crate::kraddie_avatar::kraddie_avatar;
use crate::theme::{Palette, LINE_XS, TEXT_XS};

const PREVIEW_SIDE: f32 = 56.0;
const BTN_SAVE: &str = "studio-save";
const BTN_CANCEL: &str = "studio-cancel";
const BTN_UNDO: &str = "studio-undo";
const BTN_REDO: &str = "studio-redo";
const SAVE_LABEL: &str = "Save design";
const CANCEL_LABEL: &str = "Cancel";
const UNDO_LABEL: &str = "Undo";
const REDO_LABEL: &str = "Redo";
const PREVIEW_PREFIX: &str = "studio-preview";

impl EnvironmentShell {
    pub(super) fn studio_previews(
        &self,
        studio: &KraddieStudio,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let frame = self.kraddies.frame();
        KraddieMood::ALL
            .iter()
            .enumerate()
            .map(|(index, &mood)| {
                let sprite = &studio.previews[mood.index()];
                self.kraddies.note_drawn(sprite);
                div()
                    .id((PREVIEW_PREFIX, index))
                    .flex()
                    .flex_col()
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(if studio.shown_mood() == mood {
                        pal.text_strong
                    } else {
                        pal.text_muted
                    }))
                    .child(kraddie_avatar(sprite, PREVIEW_SIDE, frame))
                    .child(mood.label())
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.edit_studio(cx, |studio| {
                            studio.mood = mood;
                            if let PaintTarget::Expression(_) = studio.target {
                                studio.target = PaintTarget::Expression(mood);
                            }
                            studio.redraw();
                        });
                    }))
                    .into_any_element()
            })
            .collect()
    }

    pub(super) fn studio_footer(
        studio: &KraddieStudio,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut footer = sheet_footer(pal);
        if studio.history.can_undo() {
            footer = footer.child(
                Button::new(BTN_UNDO)
                    .label(UNDO_LABEL)
                    .icon(IconName::Undo)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.edit_studio(cx, KraddieStudio::undo);
                    })),
            );
        }
        if studio.history.can_redo() {
            footer = footer.child(
                Button::new(BTN_REDO)
                    .label(REDO_LABEL)
                    .icon(IconName::Repeat)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.edit_studio(cx, KraddieStudio::redo);
                    })),
            );
        }
        footer
            .child(div().flex_1())
            .child(
                Button::new(BTN_CANCEL)
                    .label(CANCEL_LABEL)
                    .variant(ButtonVariant::Secondary)
                    .on_click(
                        cx.listener(|this: &mut Self, _, _, cx| this.close_kraddie_studio(cx)),
                    ),
            )
            .child(
                Button::new(BTN_SAVE)
                    .label(SAVE_LABEL)
                    .variant(ButtonVariant::Primary)
                    .on_click(
                        cx.listener(|this: &mut Self, _, _, cx| this.save_kraddie_studio(cx)),
                    ),
            )
            .into_any_element()
    }
}
