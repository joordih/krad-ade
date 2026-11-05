use gpui_kit::{Context, KeyDownEvent, Window};

use super::super::EnvironmentShell;
use super::{MenuAction, MenuCreate, MenuEntry, MenuSearch, Submenu};
use crate::typed_text::typed_text;

const KEY_ESCAPE: &str = "escape";
const KEY_BACKSPACE: &str = "backspace";
const KEY_ENTER: &str = "enter";
const KEY_UP: &str = "up";
const KEY_DOWN: &str = "down";

pub(super) fn matching<'a>(entries: &'a [MenuEntry], query: &str) -> Vec<&'a MenuEntry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return entries.iter().collect();
    }
    entries
        .iter()
        .filter(|entry| match entry {
            MenuEntry::Item {
                label,
                detail,
                trailing,
                ..
            } => [Some(label), detail.as_ref(), trailing.as_ref()]
                .into_iter()
                .flatten()
                .any(|text| text.to_lowercase().contains(&query)),
            MenuEntry::Submenu(_)
            | MenuEntry::Header(_)
            | MenuEntry::Note(_)
            | MenuEntry::Separator => false,
        })
        .collect()
}

fn create_row(
    create: Option<MenuCreate>,
    entries: &[MenuEntry],
    query: &str,
) -> Option<(String, MenuAction)> {
    let create = create?;
    let query = query.trim();
    let taken = entries.iter().any(
        |entry| matches!(entry, MenuEntry::Item { label, .. } if label.eq_ignore_ascii_case(query)),
    );
    (!query.is_empty() && !taken).then(|| (create.label(query), create.action(query)))
}

fn choices(entries: &[MenuEntry], create: Option<MenuCreate>, query: &str) -> Vec<MenuAction> {
    matching(entries, query)
        .into_iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item { action, .. } => Some(action.clone()),
            _ => None,
        })
        .chain(create_row(create, entries, query).map(|(_, action)| action))
        .collect()
}

fn choice_at(
    entries: &[MenuEntry],
    create: Option<MenuCreate>,
    query: &str,
    chosen: usize,
) -> Option<MenuAction> {
    choices(entries, create, query).into_iter().nth(chosen)
}

impl Submenu {
    pub(super) fn create_row(&self, query: &str) -> Option<(String, MenuAction)> {
        create_row(self.create, &self.entries, query)
    }
}

impl MenuSearch {
    pub(super) fn create_row(
        &self,
        entries: &[MenuEntry],
        query: &str,
    ) -> Option<(String, MenuAction)> {
        create_row(self.create, entries, query)
    }
}

impl EnvironmentShell {
    fn open_submenu_of(&self) -> Option<&Submenu> {
        let menu = self.context_menu.as_ref()?;
        match menu.entries.get(menu.submenu?)? {
            MenuEntry::Submenu(submenu) => Some(submenu),
            _ => None,
        }
    }

    pub(in crate::environment_shell) fn open_submenu(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(menu) = self.context_menu.as_mut() {
            if menu.submenu != Some(index) {
                menu.submenu = Some(index);
                menu.query.clear();
                menu.chosen = 0;
                cx.notify();
            }
        }
    }

    pub(super) fn close_submenu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.context_menu.as_mut() {
            if menu.submenu.is_some() {
                menu.submenu = None;
                menu.query.clear();
                menu.chosen = 0;
                cx.notify();
            }
        }
    }

    pub(super) fn on_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let key = event.keystroke.key.as_str();
        if key == KEY_ESCAPE {
            if self.open_submenu_of().is_some() {
                self.close_submenu(cx);
            } else {
                self.close_menu(cx);
            }
            return;
        }
        let Some(menu) = self.context_menu.as_ref() else {
            return;
        };
        let target = match self.open_submenu_of() {
            Some(submenu) => submenu
                .search
                .map(|_| (submenu.entries.as_slice(), submenu.create)),
            None => menu
                .search
                .as_ref()
                .map(|search| (menu.entries.as_slice(), search.create)),
        };
        let Some((entries, create)) = target else {
            return;
        };
        if key == KEY_ENTER {
            if let Some(action) = choice_at(entries, create, &menu.query, menu.chosen) {
                self.run_menu_action(action, window, cx);
            }
            return;
        }
        let count = choices(entries, create, &menu.query).len();
        let modifiers = event.keystroke.modifiers;
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        if count > 0 && (key == KEY_DOWN || key == KEY_UP) {
            menu.chosen = if key == KEY_DOWN {
                (menu.chosen + 1) % count
            } else {
                (menu.chosen + count - 1) % count
            };
            cx.notify();
            return;
        }
        menu.chosen = 0;
        if key == KEY_BACKSPACE {
            menu.query.pop();
        } else if let Some(typed) = typed_text(&event.keystroke, window.capslock())
            .filter(|_| !modifiers.platform && !modifiers.control)
        {
            menu.query.push_str(&typed);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::super::MenuCreate;
    use super::*;
    use krad_domain::GroupId;

    fn item(label: &str, detail: &str) -> MenuEntry {
        MenuEntry::item(label, None, MenuAction::NewGroup).with_detail(detail)
    }

    fn labels(entries: &[&MenuEntry]) -> Vec<String> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item { label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_query_matches_labels_and_details_ignoring_case() {
        let entries = vec![
            item("api", "main"),
            MenuEntry::Separator,
            item("web", "feature/login"),
        ];
        assert_eq!(matching(&entries, "").len(), 3);
        assert_eq!(labels(&matching(&entries, "API")), ["api"]);
        assert_eq!(labels(&matching(&entries, "login")), ["web"]);
        assert_eq!(labels(&matching(&entries, "zzz")), Vec::<String>::new());
    }

    #[test]
    fn only_a_new_name_offers_to_be_created() {
        let submenu = Submenu {
            label: "Switch worktree".to_owned(),
            icon: None,
            entries: vec![item("api", "main")],
            search: Some(""),
            create: Some(MenuCreate::Worktree(GroupId(1))),
        };
        assert!(submenu.create_row("API").is_none());
        assert!(submenu.create_row("  ").is_none());
        assert_eq!(
            submenu.create_row("login").map(|(_, action)| action),
            Some(MenuAction::CreateWorktree(GroupId(1), "login".to_owned()))
        );
    }

    #[test]
    fn the_tab_picker_creates_a_terminal_only_for_a_new_name() {
        let pane = krad_domain::PaneId(2);
        let search = MenuSearch {
            placeholder: "",
            create: Some(MenuCreate::Tab(pane)),
            footer: Vec::new(),
        };
        let tabs = vec![item("zsh", "")];
        assert!(search.create_row(&tabs, "ZSH").is_none());
        assert_eq!(
            choice_at(&tabs, search.create, "build", 0),
            Some(MenuAction::CreateTab(pane, "build".to_owned()))
        );
        assert_eq!(
            choice_at(&tabs, search.create, "zs", 0),
            Some(MenuAction::NewGroup)
        );
        assert_eq!(
            choice_at(&tabs, search.create, "zs", 1),
            Some(MenuAction::CreateTab(pane, "zs".to_owned()))
        );
        assert_eq!(choice_at(&tabs, search.create, "zs", 2), None);
    }
}
