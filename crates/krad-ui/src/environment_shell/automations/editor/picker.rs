use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};

use super::super::super::EnvironmentShell;
use super::super::model::{cue_root, cue_shelf, CueEntry, CueShelf};
use super::{AutomationEditor, CuePicker};
use crate::components::motion::{pop, slide_from_side};
use crate::components::{icon, Button, IconName};
use crate::theme::{
    popover_shadow, Palette, ICON_SM, LINE_XS, RADIUS_LG, RADIUS_MD, SPACE_0_5, SPACE_1, SPACE_2,
    TEXT_SM, TEXT_XS,
};

const PICKER_PREFIX: &str = "automation-cue-picker";
const LIST_PREFIX: &str = "automation-cue-list";
const ENTRY_PREFIX: &str = "automation-cue-entry";
const BTN_BACK: &str = "automation-cue-back";
const PICKER_MAX_W: f32 = 440.0;
const SLIDE_DISTANCE: f32 = 18.0;
const LOCKED_OPACITY: f32 = 0.55;
const PRESSED_OPACITY: f32 = 0.8;
const ROOT_TITLE: &str = "Pick what wakes it";
const BACK_LABEL: &str = "All cues";

const fn shelf_index(shelf: Option<CueShelf>) -> u64 {
    match shelf {
        None => 0,
        Some(CueShelf::Clock) => 1,
        Some(CueShelf::Host(krad_domain::CodeHost::Github)) => 2,
        Some(CueShelf::Host(krad_domain::CodeHost::Gitlab)) => 3,
        Some(CueShelf::App) => 4,
    }
}

impl EnvironmentShell {
    pub(super) fn cue_picker(
        &self,
        editor: &AutomationEditor,
        picker: CuePicker,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entries = match picker.shelf {
            None => cue_root(&editor.draft, self.automations.logins),
            Some(chosen) => cue_shelf(chosen, &editor.draft),
        };
        let rows: Vec<AnyElement> = entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| Self::cue_entry(index, entry, pal, cx))
            .collect();
        let header = match picker.shelf {
            None => div()
                .px(px(SPACE_2))
                .py(px(SPACE_1))
                .text_size(px(TEXT_XS))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(pal.text_muted))
                .child(ROOT_TITLE)
                .into_any_element(),
            Some(chosen) => div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_1))
                .child(
                    Button::new(BTN_BACK)
                        .label(BACK_LABEL)
                        .icon(IconName::ChevronLeft)
                        .small()
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.edit_automation_draft(cx, |editor| editor.browse_cues(None));
                        })),
                )
                .child(
                    div()
                        .text_size(px(TEXT_XS))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(pal.text_strong))
                        .child(chosen.label()),
                )
                .into_any_element(),
        };
        let distance = if picker.shelf.is_some() {
            SLIDE_DISTANCE
        } else {
            -SLIDE_DISTANCE
        };
        let list_key =
            picker.opened * (shelf_index(Some(CueShelf::App)) + 1) + shelf_index(picker.shelf);
        let list = slide_from_side(
            ElementId::NamedInteger(LIST_PREFIX.into(), list_key),
            distance,
            div()
                .relative()
                .flex()
                .flex_col()
                .gap(px(SPACE_0_5))
                .children(rows),
        );
        pop(
            ElementId::NamedInteger(PICKER_PREFIX.into(), picker.opened),
            div()
                .relative()
                .flex()
                .flex_col()
                .w_full()
                .max_w(px(PICKER_MAX_W))
                .gap(px(SPACE_1))
                .p(px(SPACE_1))
                .rounded(px(RADIUS_LG))
                .border_1()
                .border_color(pal.divider)
                .bg(rgb(pal.bg_surface))
                .shadow(popover_shadow(pal, cx))
                .overflow_hidden()
                .child(header)
                .child(list),
        )
    }

    fn cue_entry(
        index: usize,
        entry: CueEntry,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = ElementId::NamedInteger(
            ENTRY_PREFIX.into(),
            u64::try_from(index).unwrap_or_default(),
        );
        let row = |glyph: IconName, label: SharedString, detail: Option<&'static str>| {
            div()
                .id(id.clone())
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .px(px(SPACE_2))
                .py(px(SPACE_1))
                .rounded(px(RADIUS_MD))
                .cursor_pointer()
                .hover(|style| style.bg(pal.bg_hover))
                .active(|style| style.opacity(PRESSED_OPACITY))
                .child(icon(glyph, ICON_SM, pal.text_subtle))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(TEXT_SM))
                                .text_color(rgb(pal.text_strong))
                                .child(label),
                        )
                        .children(detail.map(|detail| {
                            div()
                                .text_size(px(TEXT_XS))
                                .line_height(px(LINE_XS))
                                .text_color(rgb(pal.text_muted))
                                .child(detail)
                        })),
                )
        };
        match entry {
            CueEntry::Shelf {
                shelf,
                label,
                detail,
                glyph,
                locked,
            } => row(glyph, label.into(), Some(detail))
                .when(locked, |row| row.opacity(LOCKED_OPACITY))
                .child(icon(
                    if locked {
                        IconName::Settings
                    } else {
                        IconName::ChevronRight
                    },
                    ICON_SM,
                    pal.text_muted,
                ))
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    if locked {
                        this.open_settings(window, cx);
                        return;
                    }
                    this.edit_automation_draft(cx, |editor| editor.browse_cues(Some(shelf)));
                }))
                .into_any_element(),
            CueEntry::Direct {
                label,
                detail,
                glyph,
                picked,
                pick,
            } => row(glyph, label.into(), detail)
                .when(picked, |row| {
                    row.child(icon(IconName::Tick, ICON_SM, pal.accent))
                })
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.run_automation_menu(pick.clone(), window, cx);
                }))
                .into_any_element(),
        }
    }
}
