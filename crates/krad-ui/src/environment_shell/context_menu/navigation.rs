use gpui_kit::{Context, KeyDownEvent, Window};

use super::super::EnvironmentShell;
use super::search::choices;
use super::{MenuAction, MenuEntry, MenuLevel, Submenu};
use crate::typed_text::typed_text;

const KEY_ESCAPE: &str = "escape";
const KEY_BACKSPACE: &str = "backspace";
const KEY_ENTER: &str = "enter";
const KEY_SPACE: &str = "space";
const KEY_UP: &str = "up";
const KEY_DOWN: &str = "down";
const KEY_LEFT: &str = "left";
const KEY_RIGHT: &str = "right";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Space,
    Escape,
    Backspace,
}

impl MenuKey {
    pub(super) fn of(key: &str) -> Option<Self> {
        match key {
            KEY_UP => Some(Self::Up),
            KEY_DOWN => Some(Self::Down),
            KEY_LEFT => Some(Self::Left),
            KEY_RIGHT => Some(Self::Right),
            KEY_ENTER => Some(Self::Enter),
            KEY_SPACE => Some(Self::Space),
            KEY_ESCAPE => Some(Self::Escape),
            KEY_BACKSPACE => Some(Self::Backspace),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Next,
    Previous,
}

const fn is_selectable(entry: &MenuEntry) -> bool {
    matches!(
        entry,
        MenuEntry::Item {
            disabled: false,
            ..
        } | MenuEntry::Submenu(_)
    )
}

#[must_use]
pub(super) fn step_cursor(entries: &[MenuEntry], from: Option<usize>, step: Step) -> Option<usize> {
    let selectable: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| is_selectable(entry))
        .map(|(index, _)| index)
        .collect();
    let (first, last) = (selectable.first()?, selectable.last()?);
    let Some(from) = from else {
        return Some(match step {
            Step::Next => *first,
            Step::Previous => *last,
        });
    };
    match step {
        Step::Next => selectable
            .iter()
            .find(|index| **index > from)
            .or(Some(first))
            .copied(),
        Step::Previous => selectable
            .iter()
            .rev()
            .find(|index| **index < from)
            .or(Some(last))
            .copied(),
    }
}

#[must_use]
pub(super) fn cycled(chosen: usize, count: usize, step: Step) -> usize {
    if count == 0 {
        return 0;
    }
    match step {
        Step::Next => (chosen + 1) % count,
        Step::Previous => (chosen + count - 1) % count,
    }
}

#[must_use]
pub(super) fn shortcut_action(entries: &[MenuEntry], typed: &str) -> Option<MenuAction> {
    let mut letters = typed.chars();
    let letter = letters.next().filter(|_| letters.next().is_none())?;
    entries.iter().find_map(|entry| match entry {
        MenuEntry::Item {
            key: Some(key),
            action,
            disabled: false,
            ..
        } if key.eq_ignore_ascii_case(&letter) => Some(action.clone()),
        _ => None,
    })
}

fn root_choice(index: usize, entry: &MenuEntry, key: MenuKey) -> Option<RootChoice> {
    match (entry, key) {
        (MenuEntry::Submenu(_), MenuKey::Right | MenuKey::Enter) => Some(RootChoice::Open(index)),
        (
            MenuEntry::Item {
                action,
                disabled: false,
                ..
            },
            MenuKey::Enter,
        ) => Some(RootChoice::Run(action.clone())),
        (
            MenuEntry::Item {
                action,
                disabled: false,
                ..
            },
            MenuKey::Space,
        ) if entry.is_switch() => Some(RootChoice::Run(action.clone())),
        _ => None,
    }
}

struct ListTarget {
    choices: Vec<MenuAction>,
    searchable: bool,
}

#[derive(Debug, PartialEq)]
enum RootChoice {
    Run(MenuAction),
    Open(usize),
}

impl EnvironmentShell {
    pub(super) fn open_submenu_of(&self) -> Option<&Submenu> {
        let menu = self.context_menu.as_ref()?;
        match menu.entries.get(menu.submenu?)? {
            MenuEntry::Submenu(submenu) => Some(submenu),
            _ => None,
        }
    }

    fn list_target(&self) -> Option<ListTarget> {
        let menu = self.context_menu.as_ref()?;
        if let Some(submenu) = self.open_submenu_of() {
            let searchable = submenu.search.is_some();
            if searchable || menu.level == MenuLevel::Submenu {
                return Some(ListTarget {
                    choices: choices(&submenu.entries, submenu.create, &menu.query),
                    searchable,
                });
            }
        }
        menu.search.as_ref().map(|search| ListTarget {
            choices: choices(&menu.entries, search.create, &menu.query),
            searchable: true,
        })
    }

    pub(super) fn on_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let key = MenuKey::of(event.keystroke.key.as_str());
        match key {
            Some(MenuKey::Escape | MenuKey::Left) if self.open_submenu_of().is_some() => {
                self.close_submenu(cx);
                return;
            }
            Some(MenuKey::Escape) => {
                self.close_menu(window, cx);
                return;
            }
            _ => {}
        }
        let modifiers = event.keystroke.modifiers;
        let typed = typed_text(&event.keystroke, window.capslock())
            .filter(|_| !modifiers.platform && !modifiers.control);
        match self.list_target() {
            Some(target) => self.on_list_key(key, typed.as_deref(), &target, window, cx),
            None => self.on_root_key(key, typed.as_deref(), window, cx),
        }
    }

    fn on_list_key(
        &mut self,
        key: Option<MenuKey>,
        typed: Option<&str>,
        target: &ListTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        let count = target.choices.len();
        match key {
            Some(MenuKey::Enter) => {
                if let Some(action) = target.choices.get(menu.chosen).cloned() {
                    self.run_menu_action(action, window, cx);
                }
                return;
            }
            Some(MenuKey::Down) => menu.chosen = cycled(menu.chosen, count, Step::Next),
            Some(MenuKey::Up) => menu.chosen = cycled(menu.chosen, count, Step::Previous),
            Some(MenuKey::Backspace) if target.searchable => {
                menu.query.pop();
                menu.chosen = 0;
            }
            Some(MenuKey::Space) | None if target.searchable => {
                let Some(typed) = typed else {
                    return;
                };
                menu.query.push_str(typed);
                menu.chosen = 0;
            }
            _ => return,
        }
        cx.notify();
    }

    fn on_root_key(
        &mut self,
        key: Option<MenuKey>,
        typed: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        let step = match key {
            Some(MenuKey::Down) => Some(Step::Next),
            Some(MenuKey::Up) => Some(Step::Previous),
            _ => None,
        };
        if let Some(step) = step {
            menu.cursor = step_cursor(&menu.entries, menu.cursor, step);
            if menu.submenu.is_some() && menu.submenu != menu.cursor {
                menu.submenu = None;
            }
            cx.notify();
            return;
        }
        let pointed = menu
            .cursor
            .and_then(|index| root_choice(index, menu.entries.get(index)?, key?));
        let shortcut = typed.and_then(|typed| shortcut_action(&menu.entries, typed));
        match (pointed, shortcut) {
            (Some(RootChoice::Open(index)), _) => self.enter_submenu(index, cx),
            (Some(RootChoice::Run(action)), _) | (None, Some(action)) => {
                self.run_menu_action(action, window, cx);
            }
            (None, None) => {}
        }
    }

    fn enter_submenu(&mut self, index: usize, cx: &mut Context<Self>) {
        self.open_submenu(index, cx);
        if let Some(menu) = self.context_menu.as_mut() {
            menu.level = MenuLevel::Submenu;
            menu.chosen = 0;
        }
        cx.notify();
    }

    pub(super) fn point_menu_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        if menu.cursor != Some(index) || menu.level != MenuLevel::Root {
            menu.cursor = Some(index);
            menu.level = MenuLevel::Root;
            cx.notify();
        }
    }

    pub(super) fn choose_menu_rank(
        &mut self,
        rank: usize,
        level: MenuLevel,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        if menu.chosen != rank || menu.level != level {
            menu.chosen = rank;
            menu.level = level;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::IconName;

    fn menu() -> Vec<MenuEntry> {
        vec![
            MenuEntry::Header("Open with".to_owned()),
            MenuEntry::item("New agent", None, MenuAction::NewAgent).with_key('n'),
            MenuEntry::Separator,
            MenuEntry::item("Settings", None, MenuAction::OpenSettings).disabled_when(true),
            MenuEntry::submenu("More", IconName::Repeat, Vec::new()),
            MenuEntry::item("New group", None, MenuAction::NewGroup).with_key('g'),
        ]
    }

    #[test]
    fn keys_are_read_from_their_names() {
        assert_eq!(MenuKey::of("down"), Some(MenuKey::Down));
        assert_eq!(MenuKey::of("left"), Some(MenuKey::Left));
        assert_eq!(MenuKey::of("escape"), Some(MenuKey::Escape));
        assert_eq!(MenuKey::of("space"), Some(MenuKey::Space));
        assert_eq!(MenuKey::of("a"), None);
    }

    #[test]
    fn space_flips_a_switch_row_but_not_a_plain_row() {
        let switch = MenuEntry::switch(
            "Compact view",
            IconName::ListView,
            false,
            MenuAction::ToggleSidebarDensity,
        );
        let plain = MenuEntry::item("New group", None, MenuAction::NewGroup);
        assert_eq!(
            root_choice(0, &switch, MenuKey::Space),
            Some(RootChoice::Run(MenuAction::ToggleSidebarDensity))
        );
        assert_eq!(
            root_choice(0, &switch, MenuKey::Enter),
            Some(RootChoice::Run(MenuAction::ToggleSidebarDensity))
        );
        assert_eq!(root_choice(0, &plain, MenuKey::Space), None);
    }

    #[test]
    fn arrows_skip_headers_separators_and_disabled_rows() {
        let entries = menu();
        assert_eq!(step_cursor(&entries, None, Step::Next), Some(1));
        assert_eq!(step_cursor(&entries, Some(1), Step::Next), Some(4));
        assert_eq!(step_cursor(&entries, Some(4), Step::Next), Some(5));
        assert_eq!(step_cursor(&entries, Some(4), Step::Previous), Some(1));
    }

    #[test]
    fn arrows_wrap_around_both_ends() {
        let entries = menu();
        assert_eq!(step_cursor(&entries, Some(5), Step::Next), Some(1));
        assert_eq!(step_cursor(&entries, Some(1), Step::Previous), Some(5));
        assert_eq!(step_cursor(&entries, None, Step::Previous), Some(5));
    }

    #[test]
    fn a_menu_without_choices_has_no_cursor() {
        let entries = vec![MenuEntry::Separator, MenuEntry::Header("Empty".to_owned())];
        assert_eq!(step_cursor(&entries, None, Step::Next), None);
    }

    #[test]
    fn list_choices_cycle_and_an_empty_list_stays_put() {
        assert_eq!(cycled(0, 3, Step::Next), 1);
        assert_eq!(cycled(2, 3, Step::Next), 0);
        assert_eq!(cycled(0, 3, Step::Previous), 2);
        assert_eq!(cycled(0, 0, Step::Next), 0);
    }

    #[test]
    fn a_typed_letter_runs_the_row_with_that_shortcut() {
        let entries = menu();
        assert_eq!(shortcut_action(&entries, "G"), Some(MenuAction::NewGroup));
        assert_eq!(shortcut_action(&entries, "n"), Some(MenuAction::NewAgent));
        assert_eq!(shortcut_action(&entries, "x"), None);
        assert_eq!(shortcut_action(&entries, "ng"), None);
    }
}
