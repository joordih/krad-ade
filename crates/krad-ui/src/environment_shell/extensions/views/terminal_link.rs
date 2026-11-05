use gpui_kit::{
    div, px, rgb, AnyView, AppContext as _, Context, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use krad_domain::TerminalId;
use krad_extensions::api::TerminalHandle;

use super::super::super::EnvironmentShell;
use super::super::SlotKey;
use super::ViewState;
use crate::components::{icon, IconName};
use crate::theme::{palette, ICON_SM, LINE_SM, RADIUS_XS, SPACE_2, TEXT_SM};

const LINK_ID: &str = "extension-view-terminal";
const OPEN_HINT: &str = "Show terminal";

pub(in crate::environment_shell) struct TerminalLink {
    shell: WeakEntity<EnvironmentShell>,
    terminal: TerminalId,
    title: String,
}

impl Render for TerminalLink {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = palette(cx);
        let (shell, terminal) = (self.shell.clone(), self.terminal);
        div()
            .id(LINK_ID)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .p(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_default))
            .hover(|style| style.bg(pal.bg_hover))
            .on_click(move |_, window, cx| {
                shell
                    .update(cx, |shell, cx| {
                        shell.show_terminal_tab(terminal, window, cx);
                    })
                    .unwrap_or_default();
            })
            .child(icon(IconName::Terminal, ICON_SM, pal.text_muted))
            .child(self.title.clone())
            .child(div().text_color(rgb(pal.text_muted)).child(OPEN_HINT))
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn attach_view_terminal(
        &mut self,
        key: &SlotKey,
        handle: TerminalHandle,
        terminal: TerminalId,
        title: String,
        cx: &mut Context<Self>,
    ) {
        let shell = cx.entity().downgrade();
        let link: AnyView = cx
            .new(|_| TerminalLink {
                shell,
                terminal,
                title,
            })
            .into();
        self.extensions
            .view_terminals
            .push((key.clone(), handle, link.clone()));
        let shown: Vec<_> = self
            .extensions
            .views
            .views
            .values()
            .filter(|open| open.key == *key)
            .filter_map(|open| match &open.state {
                ViewState::Shown { entity, .. } => Some(entity.clone()),
                ViewState::Loading | ViewState::Failed(_) => None,
            })
            .collect();
        for entity in shown {
            let link = link.clone();
            entity.update(cx, |view, cx| view.set_terminal(handle, link, cx));
        }
    }
}
