use gpui_kit::{Context, KeyDownEvent, Window};

use super::super::EnvironmentShell;
use super::{MenuAction, MenuEntry, Submenu};

const KEY_ESCAPE: &str = "escape";
const KEY_BACKSPACE: &str = "backspace";
const KEY_ENTER: &str = "enter";

pub(super) fn matching<'a>(entries: &'a [MenuEntry], query: &str) -> Vec<&'a MenuEntry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return entries.iter().collect();
    }
    entries
        .iter()
        .filter(|entry| match entry {
            MenuEntry::Item { label, detail, .. } => {
                label.to_lowercase().contains(&query)
                    || detail
                        .as_ref()
                        .is_some_and(|detail| detail.to_lowercase().contains(&query))
            }
            MenuEntry::Submenu(_)
            | MenuEntry::Header(_)
            | MenuEntry::Note(_)
            | MenuEntry::Separator => false,
        })
        .collect()
}

impl Submenu {
    pub(super) fn create_row(&self, query: &str) -> Option<(String, MenuAction)> {
        let create = self.create?;
        let query = query.trim();
        let taken = self.entries.iter().any(|entry| {
            matches!(entry, MenuEntry::Item { label, .. } if label.eq_ignore_ascii_case(query))
        });
        (!query.is_empty() && !taken).then(|| (create.label(query), create.action(query)))
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

    pub(super) fn open_submenu(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(menu) = self.context_menu.as_mut() {
            if menu.submenu != Some(index) {
                menu.submenu = Some(index);
                menu.query.clear();
                cx.notify();
            }
        }
    }

    pub(super) fn close_submenu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.context_menu.as_mut() {
            if menu.submenu.is_some() {
                menu.submenu = None;
                menu.query.clear();
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
        let Some(submenu) = self
            .open_submenu_of()
            .filter(|submenu| submenu.search.is_some())
        else {
            return;
        };
        let query = self
            .context_menu
            .as_ref()
            .map_or_default(|menu| menu.query.clone());
        if key == KEY_ENTER {
            let first = matching(&submenu.entries, &query)
                .into_iter()
                .find_map(|entry| match entry {
                    MenuEntry::Item { action, .. } => Some(action.clone()),
                    _ => None,
                })
                .or_else(|| submenu.create_row(&query).map(|(_, action)| action));
            if let Some(action) = first {
                self.run_menu_action(action, window, cx);
            }
            return;
        }
        let modifiers = event.keystroke.modifiers;
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        if key == KEY_BACKSPACE {
            menu.query.pop();
        } else if let Some(typed) = event
            .keystroke
            .key_char
            .as_ref()
            .filter(|_| !modifiers.platform && !modifiers.control)
        {
            menu.query.push_str(typed);
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
}
