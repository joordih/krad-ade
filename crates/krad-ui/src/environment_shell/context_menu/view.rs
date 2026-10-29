use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, App, Context, Div, ElementId, FontWeight, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, ParentElement, StatefulInteractiveElement, Styled,
    Window,
};

use super::super::EnvironmentShell;
use super::search::matching;
use super::{ContextMenu, MenuEntry, MenuPlacement, Submenu};
use crate::components::{icon, IconName};
use crate::theme::{
    palette, popover_fill, popover_shadow, Palette, ICON, ICON_SM, LINE_BASE, LINE_SM, LINE_XS,
    SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_BASE, TEXT_SM, TEXT_XS,
};

const MENU_W: f32 = 264.0;
const SUBMENU_W: f32 = 320.0;
const ITEM_H: f32 = 34.0;
const ITEM_DETAIL_H: f32 = 48.0;
const HEADER_H: f32 = 24.0;
const NOTE_H: f32 = 52.0;
const SEARCH_H: f32 = 44.0;
const SEPARATOR_H: f32 = 9.0;
const SEPARATOR_LINE: f32 = 1.0;
const SUBMENU_GAP: f32 = 6.0;
const EDGE_MARGIN: f32 = 8.0;
const PANEL_PAD: f32 = SPACE_1_5;
const PANEL_RADIUS: f32 = 14.0;
const ROW_RADIUS: f32 = 9.0;
const CARET_W: f32 = 1.5;
const CARET_H: f32 = 18.0;
const MENU_ID: &str = "context-menu";
const SUBMENU_ID: &str = "context-submenu";
const ITEM_PREFIX: &str = "menu-item";
const SUB_ITEM_PREFIX: &str = "submenu-item";
const CREATE_ID: &str = "submenu-create";
const NO_MATCHES_TEXT: &str = "No matches";
const OPTIONS_ID: &str = "menu-options";
const OPTION_PREFIX: &str = "menu-option";
const OPTIONS_W: f32 = 180.0;

const fn entry_height(entry: &MenuEntry) -> f32 {
    match entry {
        MenuEntry::Item {
            detail: Some(_), ..
        } => ITEM_DETAIL_H,
        MenuEntry::Item { .. } | MenuEntry::Submenu(_) => ITEM_H,
        MenuEntry::Header(_) => HEADER_H,
        MenuEntry::Note(_) => NOTE_H,
        MenuEntry::Separator => SEPARATOR_H,
    }
}

fn panel(pal: &Palette, cx: &App) -> Div {
    div()
        .absolute()
        .flex()
        .flex_col()
        .p(px(PANEL_PAD))
        .rounded(px(PANEL_RADIUS))
        .bg(popover_fill(pal, cx))
        .border_1()
        .border_color(pal.divider)
        .shadow(popover_shadow(pal, cx))
        .occlude()
}

fn row_id(prefix: &'static str, index: usize) -> ElementId {
    (prefix, u64::try_from(index).unwrap_or(u64::MAX)).into()
}

fn icon_slot(glyph: Option<IconName>, color: u32) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(ICON))
        .when_some(glyph, |slot, name| slot.child(icon(name, ICON, color)))
        .into_any_element()
}

fn item_body(entry: &MenuEntry, pal: &Palette) -> Vec<AnyElement> {
    let MenuEntry::Item {
        label,
        icon: glyph,
        checked,
        destructive,
        detail,
        trailing,
        ..
    } = entry
    else {
        return Vec::new();
    };
    let (text, mark) = if *destructive {
        (pal.error_fg, pal.error_fg)
    } else {
        (pal.text_strong, pal.text_muted)
    };
    vec![
        icon_slot(*glyph, mark),
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .truncate()
                    .text_size(px(TEXT_BASE))
                    .line_height(px(LINE_BASE))
                    .text_color(rgb(text))
                    .child(label.clone()),
            )
            .children(detail.as_ref().map(|detail| {
                div()
                    .truncate()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_muted))
                    .child(detail.clone())
            }))
            .into_any_element(),
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_muted))
            .children(trailing.clone())
            .when(*checked, |end| {
                end.child(icon(IconName::Tick, ICON_SM, pal.text_strong))
            })
            .into_any_element(),
    ]
}

fn row_shell(
    id: ElementId,
    height: f32,
    highlighted: bool,
    pal: &Palette,
) -> gpui_kit::Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_3))
        .h(px(height))
        .px(px(SPACE_2))
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .when(highlighted, |row| row.bg(pal.bg_hover))
        .when(!highlighted, |row| {
            row.hover(|style| style.bg(pal.bg_hover))
        })
}

fn passive_row(entry: &MenuEntry, pal: &Palette) -> AnyElement {
    match entry {
        MenuEntry::Header(text) => div()
            .flex_none()
            .flex()
            .items_end()
            .h(px(HEADER_H))
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
            .h(px(SEPARATOR_H))
            .px(px(SPACE_2))
            .child(div().h(px(SEPARATOR_LINE)).w_full().bg(pal.divider))
            .into_any_element(),
    }
}

impl EnvironmentShell {
    fn menu_row(
        index: usize,
        entry: &MenuEntry,
        open: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match entry {
            MenuEntry::Item { action, .. } => {
                let action = action.clone();
                row_shell(row_id(ITEM_PREFIX, index), entry_height(entry), false, pal)
                    .on_hover(cx.listener(|this: &mut Self, hovered: &bool, _, cx| {
                        if *hovered {
                            this.close_submenu(cx);
                        }
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.run_menu_action(action.clone(), window, cx);
                    }))
                    .children(item_body(entry, pal))
                    .into_any_element()
            }
            MenuEntry::Submenu(submenu) => row_shell(row_id(ITEM_PREFIX, index), ITEM_H, open, pal)
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    if *hovered {
                        this.open_submenu(index, cx);
                    }
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.open_submenu(index, cx);
                }))
                .child(icon_slot(submenu.icon, pal.text_muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(TEXT_BASE))
                        .line_height(px(LINE_BASE))
                        .text_color(rgb(pal.text_strong))
                        .child(submenu.label.clone()),
                )
                .child(icon(IconName::ChevronRight, ICON_SM, pal.text_muted))
                .into_any_element(),
            _ => passive_row(entry, pal),
        }
    }

    fn submenu_row(
        index: usize,
        entry: &MenuEntry,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let MenuEntry::Item {
            action, options, ..
        } = entry
        else {
            return passive_row(entry, pal);
        };
        let action = action.clone();
        let options = options.clone();
        row_shell(
            row_id(SUB_ITEM_PREFIX, index),
            entry_height(entry),
            false,
            pal,
        )
        .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
            this.run_menu_action(action.clone(), window, cx);
        }))
        .when(!options.is_empty(), |row| {
            row.on_mouse_down(
                MouseButton::Right,
                cx.listener(
                    move |this: &mut Self, event: &gpui_kit::MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let at = (event.position.x.as_f32(), event.position.y.as_f32());
                        this.open_menu_options(options.clone(), at, cx);
                    },
                ),
            )
        })
        .children(item_body(entry, pal))
        .into_any_element()
    }

    fn search_row(placeholder: &'static str, query: &str, pal: &Palette) -> AnyElement {
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
            .h(px(SEARCH_H))
            .px(px(SPACE_2))
            .mb(px(PANEL_PAD))
            .border_b_1()
            .border_color(pal.divider)
            .text_size(px(TEXT_BASE))
            .line_height(px(LINE_BASE))
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

    fn submenu_panel(
        submenu: &Submenu,
        query: &str,
        (left, top): (f32, f32),
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = matching(&submenu.entries, query);
        let create = submenu.create_row(query).map(|(label, action)| {
            let entry = MenuEntry::item(label, Some(IconName::Add), action.clone());
            row_shell(CREATE_ID.into(), ITEM_H, shown.is_empty(), pal)
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.run_menu_action(action.clone(), window, cx);
                }))
                .children(item_body(&entry, pal))
                .into_any_element()
        });
        let empty = (shown.is_empty() && create.is_none()).then(|| {
            div()
                .px(px(SPACE_2))
                .py(px(SPACE_2))
                .text_size(px(TEXT_SM))
                .text_color(rgb(pal.text_muted))
                .child(NO_MATCHES_TEXT)
        });
        let rows: Vec<AnyElement> = shown
            .into_iter()
            .enumerate()
            .map(|(index, entry)| Self::submenu_row(index, entry, pal, cx))
            .collect();
        panel(pal, cx)
            .id(SUBMENU_ID)
            .left(px(left))
            .top(px(top))
            .w(px(SUBMENU_W))
            .children(
                submenu
                    .search
                    .map(|placeholder| Self::search_row(placeholder, query, pal)),
            )
            .children(rows)
            .children(create)
            .children(empty)
            .into_any_element()
    }

    fn submenu_origin(
        menu: &ContextMenu,
        menu_left: f32,
        menu_top: f32,
        root_w: f32,
    ) -> (f32, f32) {
        let index = menu.submenu.unwrap_or_default();
        let above: f32 = menu.entries.iter().take(index).map(entry_height).sum();
        let right = menu_left + MENU_W + SUBMENU_GAP;
        let left = if right + SUBMENU_W + EDGE_MARGIN <= root_w {
            right
        } else {
            (menu_left - SUBMENU_GAP - SUBMENU_W).max(EDGE_MARGIN)
        };
        (left, menu_top + above)
    }

    fn options_panel(
        options: &[MenuEntry],
        (x, y): (f32, f32),
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows: Vec<AnyElement> = options
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let MenuEntry::Item { action, .. } = entry else {
                    return passive_row(entry, pal);
                };
                let action = action.clone();
                row_shell(
                    row_id(OPTION_PREFIX, index),
                    entry_height(entry),
                    false,
                    pal,
                )
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.run_menu_action(action.clone(), window, cx);
                }))
                .children(item_body(entry, pal))
                .into_any_element()
            })
            .collect();
        panel(pal, cx)
            .id(OPTIONS_ID)
            .left(px(x))
            .top(px(y))
            .w(px(OPTIONS_W))
            .children(rows)
            .into_any_element()
    }

    pub(in crate::environment_shell) fn context_menu_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.context_menu.as_ref()?;
        let pal = palette(cx);
        let (root_w, root_h) = self.root_size(window);
        let max_height = root_h - 2.0 * EDGE_MARGIN;
        let height =
            (menu.entries.iter().map(entry_height).sum::<f32>() + 2.0 * PANEL_PAD).min(max_height);
        let (x, y) = (
            menu.origin.0 - self.frame_inset.0,
            menu.origin.1 - self.frame_inset.1,
        );
        let top = match menu.placement {
            MenuPlacement::Below => y,
            MenuPlacement::Above => y - height,
        };
        let left = x.min(root_w - MENU_W - EDGE_MARGIN).max(EDGE_MARGIN);
        let top = top.min(root_h - height - EDGE_MARGIN).max(EDGE_MARGIN);
        let rows: Vec<AnyElement> = menu
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                Self::menu_row(index, entry, menu.submenu == Some(index), pal, cx)
            })
            .collect();
        let submenu = menu
            .submenu
            .and_then(|index| match menu.entries.get(index) {
                Some(MenuEntry::Submenu(submenu)) => Some(submenu),
                _ => None,
            })
            .map(|submenu| {
                let origin = Self::submenu_origin(menu, left, top, root_w);
                Self::submenu_panel(submenu, &menu.query, origin, pal, cx)
            });
        let options = menu.options.as_ref().map(|(options, (x, y))| {
            let at = (x - self.frame_inset.0, y - self.frame_inset.1);
            Self::options_panel(options, at, pal, cx)
        });
        let dismiss = |this: &mut Self,
                       _: &gpui_kit::MouseDownEvent,
                       _: &mut Window,
                       cx: &mut Context<Self>| {
            this.close_menu(cx);
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .track_focus(&menu.focus)
                .on_key_down(
                    cx.listener(|this: &mut Self, event: &KeyDownEvent, window, cx| {
                        this.on_menu_key(event, window, cx);
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
                        .on_mouse_down(MouseButton::Right, cx.listener(dismiss)),
                )
                .child(
                    panel(pal, cx)
                        .id(MENU_ID)
                        .left(px(left))
                        .top(px(top))
                        .w(px(MENU_W))
                        .max_h(px(max_height))
                        .overflow_y_scroll()
                        .children(rows),
                )
                .children(submenu)
                .children(options)
                .into_any_element(),
        )
    }
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
}
