use gpui_kit::{point, px, Context, Window};

use super::super::EnvironmentShell;
use super::{MenuAction, MenuEntry, MenuSearch};
use crate::components::IconName;
use crate::native_picker::{picker_host, show_picker, Picker, PickerChoice, PickerRow};

const fn symbol_of(icon: IconName) -> Option<&'static str> {
    match icon {
        IconName::Add => Some("plus.rectangle.on.rectangle"),
        IconName::Terminal => Some("terminal"),
        IconName::Chat => Some("bubble.left"),
        IconName::File => Some("doc"),
        IconName::Copy => Some("doc.on.doc"),
        IconName::Folder => Some("folder"),
        IconName::Settings => Some("gearshape"),
        IconName::GitBranch => Some("arrow.triangle.branch"),
        _ => None,
    }
}

fn rows_of(entries: &[MenuEntry]) -> (Vec<PickerRow>, Vec<MenuAction>) {
    entries
        .iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item {
                label,
                icon,
                action,
                checked,
                key,
                ..
            } => Some((
                PickerRow {
                    label: label.clone(),
                    checked: *checked,
                    shortcut: *key,
                    symbol: icon.and_then(symbol_of),
                },
                action.clone(),
            )),
            _ => None,
        })
        .unzip()
}

impl EnvironmentShell {
    pub(super) fn open_native_picker(
        &mut self,
        (x, y): (f32, f32),
        entries: &[MenuEntry],
        search: &MenuSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = picker_host(window) else {
            return false;
        };
        let (rows, row_actions) = rows_of(entries);
        let (footer, footer_actions) = rows_of(&search.footer);
        let create = search.create;
        let picker = Picker {
            placeholder: search.placeholder,
            rows,
            footer,
            create_label: create.map(super::MenuCreate::verb),
        };
        let origin = point(px(x - self.frame_inset.0), px(y - self.frame_inset.1));
        cx.spawn_in(window, async move |this, cx| {
            let choice = show_picker(&picker, origin, &host);
            let action = match choice {
                Some(PickerChoice::Row(index)) => row_actions.get(index).cloned(),
                Some(PickerChoice::Footer(index)) => footer_actions.get(index).cloned(),
                Some(PickerChoice::Create(query)) => create.map(|create| create.action(&query)),
                None => None,
            };
            this.update_in(cx, |this, window, cx| {
                if let Some(action) = action {
                    this.run_menu_action(action, window, cx);
                }
                window.refresh();
            })
            .unwrap_or_default();
        })
        .detach();
        true
    }
}
