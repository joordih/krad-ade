use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, point, px, size, AnyElement, Bounds, Context, Entity, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, StatefulInteractiveElement, Styled, Window,
};

use super::super::EnvironmentShell;
use super::rows::{
    empty_row, entry_height, item_body, passive_row, row_id, row_shell, search_row, submenu_body,
    switch_look, RowLook, NOTE_H,
};
use super::search::{item_ranks, items_in, matching};
use super::{ContextMenu, MenuAction, MenuEntry, MenuLevel, MenuPlacement, MenuSearch, Submenu};
use crate::components::floating::{floating_at, floating_panel, place, Anchor, Placed, Side};
use crate::components::{copy_glyph, CopyFeedback, IconName};
use crate::theme::{
    palette, Palette, FLOATING_PAD, ICON, MENU_MIN_W, MENU_ROW_H, MENU_SEARCH_H, MENU_W,
    MENU_WIDE_W,
};

const BORDER_W: f32 = 1.0;
const MENU_ID: &str = "context-menu";
const MENU_MOTION_ID: &str = "context-menu-motion";
const SUBMENU_ID: &str = "context-submenu";
const SUBMENU_MOTION_PREFIX: &str = "context-submenu-motion";
const SUBMENU_ROWS_ID: &str = "context-submenu-rows";
const ITEM_PREFIX: &str = "menu-item";
const SUB_ITEM_PREFIX: &str = "submenu-item";
const CREATE_ID: &str = "submenu-create";
const NO_MATCHES_TEXT: &str = "No matches";
const OPTIONS_ID: &str = "menu-options";
const OPTIONS_MOTION_ID: &str = "menu-options-motion";
const OPTION_PREFIX: &str = "menu-option";

const fn chrome() -> f32 {
    2.0 * (FLOATING_PAD + BORDER_W)
}

const fn side_of(placement: MenuPlacement) -> Side {
    match placement {
        MenuPlacement::Below => Side::Below,
        MenuPlacement::Above => Side::Above,
    }
}

#[derive(Clone, Copy)]
enum RowPointer {
    Entry(usize),
    Rank(usize),
    Inert,
}

impl EnvironmentShell {
    fn create_choice(
        label: String,
        action: MenuAction,
        (chosen, rank, level): (bool, usize, MenuLevel),
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry = MenuEntry::item(label, Some(IconName::Add), action.clone());
        let look = RowLook::of(chosen);
        row_shell(CREATE_ID.into(), MENU_ROW_H, look, pal)
            .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                if *hovered {
                    this.choose_menu_rank(rank, level, cx);
                }
            }))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.run_menu_action(action.clone(), window, cx);
            }))
            .children(item_body(&entry, look, None, pal))
            .into_any_element()
    }

    fn menu_row(
        index: usize,
        entry: &MenuEntry,
        (look, pointer): (RowLook, RowPointer),
        copies: &Entity<CopyFeedback>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        match entry {
            MenuEntry::Item {
                action, disabled, ..
            } => {
                let look = if *disabled {
                    RowLook::Disabled
                } else {
                    switch_look(entry, look)
                };
                let lead = match action {
                    MenuAction::CopyChat(copy) => {
                        let color = if *disabled {
                            pal.text_disabled
                        } else {
                            pal.text_muted
                        };
                        Some(copy_glyph(copies, &copy.key.into(), ICON, color, cx))
                    }
                    _ => None,
                };
                let action = action.clone();
                let enabled = !*disabled;
                row_shell(row_id(ITEM_PREFIX, index), entry_height(entry), look, pal)
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        if !*hovered {
                            return;
                        }
                        this.close_submenu(cx);
                        match pointer {
                            RowPointer::Entry(index) if enabled => this.point_menu_at(index, cx),
                            RowPointer::Rank(rank) => {
                                this.choose_menu_rank(rank, MenuLevel::Root, cx);
                            }
                            RowPointer::Entry(_) | RowPointer::Inert => {}
                        }
                    }))
                    .when(enabled, |row| {
                        row.on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                            this.run_menu_action(action.clone(), window, cx);
                        }))
                    })
                    .children(item_body(entry, look, lead, pal))
                    .into_any_element()
            }
            MenuEntry::Submenu(submenu) => {
                row_shell(row_id(ITEM_PREFIX, index), MENU_ROW_H, look, pal)
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        if *hovered {
                            this.open_submenu(index, cx);
                            this.point_menu_at(index, cx);
                        }
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.open_submenu(index, cx);
                    }))
                    .children(submenu_body(submenu, look, pal))
                    .into_any_element()
            }
            _ => passive_row(entry, pal),
        }
    }

    fn submenu_row(
        index: usize,
        entry: &MenuEntry,
        (chosen, rank): (bool, Option<usize>),
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let MenuEntry::Item {
            action,
            options,
            disabled,
            ..
        } = entry
        else {
            return passive_row(entry, pal);
        };
        let look = if *disabled {
            RowLook::Disabled
        } else {
            RowLook::of(chosen)
        };
        let action = action.clone();
        let options = options.clone();
        row_shell(
            row_id(SUB_ITEM_PREFIX, index),
            entry_height(entry),
            look,
            pal,
        )
        .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
            if let Some(rank) = rank.filter(|_| *hovered) {
                this.choose_menu_rank(rank, MenuLevel::Submenu, cx);
            }
        }))
        .when(!*disabled, |row| {
            row.on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.run_menu_action(action.clone(), window, cx);
            }))
        })
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
        .children(item_body(entry, look, None, pal))
        .into_any_element()
    }

    fn submenu_panel(
        (index, submenu): (usize, &Submenu),
        menu: &ContextMenu,
        placed: Placed,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = menu.query.as_str();
        let shown = matching(&submenu.entries, query);
        let ranks = item_ranks(&shown);
        let listed = items_in(&shown);
        let searching = submenu.search.is_some();
        let keyed = searching || menu.level == MenuLevel::Submenu;
        let create = submenu.create_row(query).map(|(label, action)| {
            let chosen = searching && menu.chosen == listed;
            Self::create_choice(label, action, (chosen, listed, MenuLevel::Submenu), pal, cx)
        });
        let empty = (shown.is_empty() && create.is_none()).then(|| empty_row(NO_MATCHES_TEXT, pal));
        let rows: Vec<AnyElement> = shown
            .into_iter()
            .zip(ranks)
            .enumerate()
            .map(|(row, (entry, rank))| {
                let picked = keyed && rank == Some(menu.chosen);
                Self::submenu_row(row, entry, (picked, rank), pal, cx)
            })
            .collect();
        let search = submenu.search.map_or(0.0, |_| MENU_SEARCH_H);
        let rows_max = (placed.max_height - chrome() - search).max(0.0);
        let panel = floating_panel(pal, cx)
            .id(SUBMENU_ID)
            .w(px(MENU_WIDE_W))
            .children(
                submenu
                    .search
                    .map(|placeholder| search_row(placeholder, query, pal)),
            )
            .child(
                div()
                    .id(SUBMENU_ROWS_ID)
                    .flex()
                    .flex_col()
                    .max_h(px(rows_max))
                    .overflow_y_scroll()
                    .children(rows)
                    .children(create)
                    .children(empty),
            );
        floating_at(row_id(SUBMENU_MOTION_PREFIX, index), placed, panel)
    }

    fn submenu_rows_height(submenu: &Submenu, query: &str) -> f32 {
        let shown = matching(&submenu.entries, query);
        let create = submenu.create_row(query).map_or(0.0, |_| MENU_ROW_H);
        let empty = if shown.is_empty() && create == 0.0 {
            NOTE_H
        } else {
            0.0
        };
        shown.into_iter().map(entry_height).sum::<f32>() + create + empty
    }

    fn submenu_placement(
        menu: &ContextMenu,
        (index, submenu): (usize, &Submenu),
        (menu_left, menu_top): (f32, f32),
        root: (f32, f32),
    ) -> Placed {
        let above: f32 = menu.entries.iter().take(index).map(entry_height).sum();
        let row = Bounds::new(
            point(px(menu_left), px(menu_top + above)),
            size(px(MENU_W), px(MENU_ROW_H)),
        );
        let search = submenu.search.map_or(0.0, |_| MENU_SEARCH_H);
        let height = chrome() + search + Self::submenu_rows_height(submenu, &menu.query);
        place(
            Anchor::Bounds(row),
            size(px(MENU_WIDE_W), px(height)),
            size(px(root.0), px(root.1)),
            Side::After,
        )
    }

    fn options_panel(
        options: &[MenuEntry],
        at: (f32, f32),
        root: (f32, f32),
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
                    RowLook::Rest,
                    pal,
                )
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.run_menu_action(action.clone(), window, cx);
                }))
                .children(item_body(entry, RowLook::Rest, None, pal))
                .into_any_element()
            })
            .collect();
        let height = chrome() + options.iter().map(entry_height).sum::<f32>();
        let placed = place(
            Anchor::Point(point(px(at.0), px(at.1))),
            size(px(MENU_MIN_W), px(height)),
            size(px(root.0), px(root.1)),
            Side::Below,
        );
        let panel = floating_panel(pal, cx)
            .id(OPTIONS_ID)
            .w(px(MENU_MIN_W))
            .children(rows);
        floating_at(OPTIONS_MOTION_ID, placed, panel)
    }

    fn root_rows_height(menu: &ContextMenu) -> f32 {
        let Some(search) = &menu.search else {
            return menu.entries.iter().map(entry_height).sum();
        };
        let shown: f32 = matching(&menu.entries, &menu.query)
            .into_iter()
            .map(entry_height)
            .sum();
        let create = search
            .create_row(&menu.entries, &menu.query)
            .map_or(0.0, |_| MENU_ROW_H);
        let empty = if shown == 0.0 && create == 0.0 {
            NOTE_H
        } else {
            0.0
        };
        let footer: f32 = search.footer.iter().map(entry_height).sum();
        MENU_SEARCH_H + shown + create + empty + footer
    }

    fn searched_rows(
        menu: &ContextMenu,
        search: &MenuSearch,
        copies: &Entity<CopyFeedback>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        let shown = matching(&menu.entries, &menu.query);
        let ranks = item_ranks(&shown);
        let listed = items_in(&shown);
        let create = search
            .create_row(&menu.entries, &menu.query)
            .map(|(label, action)| {
                let chosen = menu.chosen == listed;
                Self::create_choice(label, action, (chosen, listed, MenuLevel::Root), pal, cx)
            });
        let empty = (shown.is_empty() && create.is_none()).then(|| empty_row(NO_MATCHES_TEXT, pal));
        let count = shown.len();
        let mut rows = vec![search_row(search.placeholder, &menu.query, pal)];
        rows.extend(
            shown
                .into_iter()
                .zip(ranks)
                .enumerate()
                .map(|(index, (entry, rank))| {
                    let look = RowLook::of(rank == Some(menu.chosen));
                    let pointer = rank.map_or(RowPointer::Inert, RowPointer::Rank);
                    Self::menu_row(index, entry, (look, pointer), copies, cx)
                }),
        );
        rows.extend(create);
        rows.extend(empty);
        rows.extend(search.footer.iter().enumerate().map(|(index, entry)| {
            Self::menu_row(
                count + 1 + index,
                entry,
                (RowLook::Rest, RowPointer::Inert),
                copies,
                cx,
            )
        }));
        rows
    }

    fn plain_rows(
        menu: &ContextMenu,
        copies: &Entity<CopyFeedback>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        menu.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let pointed = menu.cursor == Some(index) && menu.level == MenuLevel::Root;
                let look = if pointed {
                    RowLook::Chosen
                } else if menu.submenu == Some(index) {
                    RowLook::Open
                } else {
                    RowLook::Rest
                };
                Self::menu_row(index, entry, (look, RowPointer::Entry(index)), copies, cx)
            })
            .collect()
    }

    pub(in crate::environment_shell) fn context_menu_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.context_menu.as_ref()?;
        let pal = palette(cx);
        let root = self.root_size(window);
        let height = Self::root_rows_height(menu) + chrome();
        let origin = point(px(menu.origin.0), px(menu.origin.1));
        let placed = place(
            Anchor::Point(origin).relative_to(self.frame_inset),
            size(px(MENU_W), px(height)),
            size(px(root.0), px(root.1)),
            side_of(menu.placement),
        );
        let top = placed.top(root.1, height);
        let rows = match &menu.search {
            Some(search) => Self::searched_rows(menu, search, &self.copy_feedback, cx),
            None => Self::plain_rows(menu, &self.copy_feedback, cx),
        };
        let submenu = menu
            .submenu
            .and_then(|index| match menu.entries.get(index) {
                Some(MenuEntry::Submenu(submenu)) => Some((index, submenu)),
                _ => None,
            })
            .map(|opened| {
                let at = Self::submenu_placement(menu, opened, (placed.left, top), root);
                Self::submenu_panel(opened, menu, at, pal, cx)
            });
        let options = menu.options.as_ref().map(|(options, (x, y))| {
            let at = (x - self.frame_inset.0, y - self.frame_inset.1);
            Self::options_panel(options, at, root, pal, cx)
        });
        let dismiss = |this: &mut Self,
                       _: &gpui_kit::MouseDownEvent,
                       window: &mut Window,
                       cx: &mut Context<Self>| {
            this.close_menu(window, cx);
        };
        let panel = floating_panel(pal, cx)
            .id(MENU_ID)
            .w(px(MENU_W))
            .max_h(px(placed.max_height))
            .overflow_y_scroll()
            .children(rows);
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
                .child(floating_at(MENU_MOTION_ID, placed, panel))
                .children(submenu)
                .children(options)
                .into_any_element(),
        )
    }
}
