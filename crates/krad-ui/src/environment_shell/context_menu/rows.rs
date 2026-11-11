use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Stateful, StatefulInteractiveElement, Styled,
};

use super::{CheckStyle, MenuEntry, Submenu};
use crate::components::{icon, toggle_switch, IconName};
use crate::theme::{
    Palette, FLOATING_PAD, FLOATING_ROW_RADIUS, ICON, ICON_SM, LINE_SM, LINE_XS, MENU_HEADER_H,
    MENU_ROW_DETAIL_H, MENU_ROW_H, MENU_SEARCH_H, MENU_SEPARATOR_H, SPACE_1, SPACE_2, SPACE_3,
    TEXT_SM, TEXT_XS,
};

pub(super) const NOTE_H: f32 = 52.0;
const SEPARATOR_LINE: f32 = 1.0;
const CARET_W: f32 = 1.5;
const CARET_H: f32 = 16.0;
const KEYCAP_PAD_X: f32 = SPACE_1;
const SWITCH_PREFIX: &str = "menu-switch-";

pub(super) const fn entry_height(entry: &MenuEntry) -> f32 {
    match entry {
        MenuEntry::Item {
            detail: Some(_), ..
        } => MENU_ROW_DETAIL_H,
        MenuEntry::Item { .. } | MenuEntry::Submenu(_) => MENU_ROW_H,
        MenuEntry::Header(_) => MENU_HEADER_H,
        MenuEntry::Note(_) => NOTE_H,
        MenuEntry::Separator => MENU_SEPARATOR_H,
    }
}

pub(super) fn row_id(prefix: &'static str, index: usize) -> ElementId {
    (prefix, u64::try_from(index).unwrap_or(u64::MAX)).into()
}

pub(super) fn lead_slot(lead: Option<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(ICON))
        .children(lead)
        .into_any_element()
}

fn icon_slot(glyph: Option<IconName>, color: u32) -> AnyElement {
    lead_slot(glyph.map(|name| icon(name, ICON, color).into_any_element()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RowLook {
    Rest,
    Open,
    Chosen,
    Disabled,
}

impl RowLook {
    pub(super) fn of(chosen: bool) -> Self {
        if chosen {
            Self::Chosen
        } else {
            Self::Rest
        }
    }
}

struct RowInk {
    text: u32,
    mark: u32,
    secondary: u32,
}

impl RowInk {
    const fn of(look: RowLook, destructive: bool, pal: &Palette) -> Self {
        match look {
            RowLook::Disabled => Self {
                text: pal.text_disabled,
                mark: pal.text_disabled,
                secondary: pal.text_disabled,
            },
            RowLook::Chosen => Self {
                text: pal.on_accent,
                mark: pal.on_accent,
                secondary: pal.on_accent,
            },
            RowLook::Rest | RowLook::Open if destructive => Self {
                text: pal.error_fg,
                mark: pal.error_fg,
                secondary: pal.text_muted,
            },
            RowLook::Rest | RowLook::Open => Self {
                text: pal.text_strong,
                mark: pal.text_muted,
                secondary: pal.text_muted,
            },
        }
    }
}

fn keycap(key: char, color: u32) -> Div {
    div()
        .flex_none()
        .px(px(KEYCAP_PAD_X))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(color))
        .child(key.to_uppercase().to_string())
}

fn label(text: String, color: u32) -> Div {
    div()
        .truncate()
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(color))
        .child(text)
}

pub(super) fn item_body(
    entry: &MenuEntry,
    look: RowLook,
    lead: Option<AnyElement>,
    pal: &Palette,
) -> Vec<AnyElement> {
    let MenuEntry::Item {
        label: text,
        icon: glyph,
        checked,
        check_style,
        destructive,
        detail,
        trailing,
        key,
        ..
    } = entry
    else {
        return Vec::new();
    };
    let ink = RowInk::of(look, *destructive, pal);
    let switched = *check_style == CheckStyle::Switch;
    let ticked = *checked && !switched;
    let tick_leads = ticked && glyph.is_none();
    let leading = if tick_leads {
        Some(IconName::Tick)
    } else {
        *glyph
    };
    vec![
        match lead {
            Some(lead) => lead_slot(Some(lead)),
            None => icon_slot(leading, if tick_leads { ink.text } else { ink.mark }),
        },
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(label(text.clone(), ink.text))
            .children(detail.as_ref().map(|detail| {
                div()
                    .truncate()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(ink.secondary))
                    .child(detail.clone())
            }))
            .into_any_element(),
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .text_color(rgb(ink.secondary))
            .children(trailing.clone())
            .when(ticked && !tick_leads, |end| {
                end.child(icon(IconName::Tick, ICON_SM, ink.text))
            })
            .when(switched, |end| {
                end.child(toggle_switch(
                    ElementId::from(format!("{SWITCH_PREFIX}{text}")),
                    *checked,
                    pal,
                ))
            })
            .children(key.map(|key| keycap(key, ink.secondary)))
            .into_any_element(),
    ]
}

pub(super) fn submenu_body(submenu: &Submenu, look: RowLook, pal: &Palette) -> Vec<AnyElement> {
    let ink = RowInk::of(look, false, pal);
    vec![
        icon_slot(submenu.icon, ink.mark),
        label(submenu.label.clone(), ink.text)
            .flex_1()
            .min_w_0()
            .into_any_element(),
        icon(IconName::ChevronRight, ICON_SM, ink.mark).into_any_element(),
    ]
}

pub(super) fn switch_look(entry: &MenuEntry, look: RowLook) -> RowLook {
    match look {
        RowLook::Chosen if entry.is_switch() => RowLook::Open,
        other => other,
    }
}

pub(super) fn row_shell(id: ElementId, height: f32, look: RowLook, pal: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_2))
        .h(px(height))
        .px(px(SPACE_2))
        .rounded(px(FLOATING_ROW_RADIUS))
        .map(|row| match look {
            RowLook::Chosen => row.cursor_pointer().bg(rgb(pal.accent)),
            RowLook::Open => row.cursor_pointer().bg(pal.bg_hover),
            RowLook::Rest => row
                .cursor_pointer()
                .hover(|style| style.bg(pal.bg_hover))
                .active(|style| style.bg(pal.bg_selected)),
            RowLook::Disabled => row.cursor_default(),
        })
}

pub(super) fn passive_row(entry: &MenuEntry, pal: &Palette) -> AnyElement {
    match entry {
        MenuEntry::Header(text) => div()
            .flex_none()
            .flex()
            .items_end()
            .h(px(MENU_HEADER_H))
            .px(px(SPACE_2))
            .pb(px(SPACE_1 / 2.0))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(pal.text_subtle))
            .child(text.clone())
            .into_any_element(),
        MenuEntry::Note(text) => div()
            .flex_none()
            .px(px(SPACE_2))
            .py(px(SPACE_2))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.error_fg))
            .child(text.clone())
            .into_any_element(),
        _ => div()
            .flex_none()
            .flex()
            .items_center()
            .h(px(MENU_SEPARATOR_H))
            .px(px(SPACE_2))
            .child(div().h(px(SEPARATOR_LINE)).w_full().bg(pal.divider))
            .into_any_element(),
    }
}

pub(super) fn empty_row(text: &'static str, pal: &Palette) -> AnyElement {
    div()
        .px(px(SPACE_2))
        .py(px(SPACE_2))
        .text_size(px(TEXT_SM))
        .text_color(rgb(pal.text_muted))
        .child(text)
        .into_any_element()
}

pub(super) fn search_row(placeholder: &'static str, query: &str, pal: &Palette) -> AnyElement {
    let caret = div()
        .flex_none()
        .w(px(CARET_W))
        .h(px(CARET_H))
        .bg(rgb(pal.text_strong));
    let typed = (!query.is_empty()).then(|| {
        div()
            .text_color(rgb(pal.text_strong))
            .child(query.to_owned())
    });
    div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_3))
        .h(px(MENU_SEARCH_H - FLOATING_PAD))
        .px(px(SPACE_2))
        .mb(px(FLOATING_PAD))
        .rounded(px(FLOATING_ROW_RADIUS))
        .bg(pal.bg_hover)
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .child(icon(IconName::Search, ICON, pal.text_muted))
        .children(typed)
        .child(caret)
        .when(query.is_empty(), |row| {
            row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_subtle))
                    .child(placeholder),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::super::MenuAction;
    use super::*;

    #[test]
    fn a_second_line_makes_a_row_taller() {
        let plain = MenuEntry::item("Settings", None, MenuAction::NewGroup);
        let detailed = plain.clone().with_detail("Every checkout");
        assert!(entry_height(&detailed) > entry_height(&plain));
    }

    #[test]
    fn a_pointed_switch_row_keeps_its_switch_visible_on_a_quiet_fill() {
        let switch = MenuEntry::switch(
            "Branches",
            IconName::GitBranch,
            true,
            MenuAction::ToggleSidebarDensity,
        );
        let plain = MenuEntry::item("Settings", None, MenuAction::NewGroup);
        assert!(switch_look(&switch, RowLook::Chosen) == RowLook::Open);
        assert!(switch_look(&plain, RowLook::Chosen) == RowLook::Chosen);
    }
}
