use gpui_kit::{
    div, px, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use super::super::dialog_parts::{segment, SegmentSize};
use super::super::EnvironmentShell;
use crate::components::{card, IconName};
use crate::theme::{palette, SPACE_0_5, SPACE_1, SPACE_1_5};

const TAB_PREFIX: &str = "notes-sidebar-tab";
const VAULT_LABEL: &str = "Notes";
const INSPECTOR_LABEL: &str = "Inspector";
const HEADER_H: f32 = 36.0;
const RULE_H: f32 = 1.0;
const TABS: [SidebarTab; 2] = [SidebarTab::Vault, SidebarTab::Inspector];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) enum SidebarTab {
    #[default]
    Vault,
    Inspector,
}

impl SidebarTab {
    const fn label(self) -> &'static str {
        match self {
            Self::Vault => VAULT_LABEL,
            Self::Inspector => INSPECTOR_LABEL,
        }
    }

    const fn glyph(self) -> IconName {
        match self {
            Self::Vault => IconName::Book,
            Self::Inspector => IconName::Note,
        }
    }
}

pub(in crate::environment_shell) struct NotesSidebar {
    open: bool,
    tab: SidebarTab,
}

impl Default for NotesSidebar {
    fn default() -> Self {
        Self {
            open: true,
            tab: SidebarTab::Vault,
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn notes_sidebar_open(&self) -> bool {
        self.notes.sidebar.open
    }

    pub(in crate::environment_shell) fn toggle_notes_sidebar(&mut self, cx: &mut Context<Self>) {
        self.notes.sidebar.open = !self.notes.sidebar.open;
        cx.notify();
    }

    pub(super) fn toggle_note_inspector(&mut self, cx: &mut Context<Self>) {
        let showing = self.notes.inspector_open
            && self.notes.sidebar.open
            && self.notes.sidebar.tab == SidebarTab::Inspector;
        if showing {
            self.notes.inspector_open = false;
            self.notes.sidebar.tab = SidebarTab::Vault;
        } else {
            self.notes.inspector_open = true;
            self.notes.sidebar.open = true;
            self.notes.sidebar.tab = SidebarTab::Inspector;
        }
        cx.notify();
    }

    pub(super) fn notes_sidebar(
        &mut self,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.notes.sidebar.open {
            return None;
        }
        let pal = palette(cx);
        let tab = if self.notes.inspector_open {
            self.notes.sidebar.tab
        } else {
            SidebarTab::Vault
        };
        let header = self.notes.inspector_open.then(|| {
            let tabs: Vec<AnyElement> = TABS
                .iter()
                .enumerate()
                .map(|(index, shown)| {
                    let shown = *shown;
                    segment(
                        (TAB_PREFIX, index),
                        shown.label(),
                        Some(shown.glyph()),
                        shown == tab,
                        SegmentSize::Compact,
                        pal,
                        cx,
                    )
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.notes.sidebar.tab = shown;
                        cx.notify();
                    }))
                    .into_any_element()
                })
                .collect();
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(SPACE_0_5))
                .h(px(HEADER_H))
                .px(px(SPACE_1_5))
                .border_b(px(RULE_H))
                .border_color(pal.divider)
                .children(tabs)
        });
        let body = match tab {
            SidebarTab::Vault => self.vault_tree(cx),
            SidebarTab::Inspector => self.inspector(cx),
        };
        Some(
            card(pal, cx)
                .id("notes-sidebar")
                .flex()
                .flex_col()
                .flex_none()
                .w(px(width))
                .h_full()
                .min_h_0()
                .pb(px(SPACE_1))
                .children(header)
                .child(div().flex().flex_col().flex_1().min_h_0().child(body))
                .into_any_element(),
        )
    }
}
