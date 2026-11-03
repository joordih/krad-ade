use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::ToolchainScope;
use std::path::Path;

use super::super::context_menu::MenuPlacement;
use super::super::language_servers::Document;
use super::super::{EnvironmentShell, LiveEnv};
use super::label::status_label;
use crate::components::tooltip_view;
use crate::theme::{palette, LINE_XS, RADIUS_XS, SPACE_0_5, SPACE_1, SPACE_2, TEXT_XS};

const STATUS_ITEM_ID: &str = "toolchain-status";
const CHOOSE_HINT: &str = "Click to choose the toolchain the editor analyses this project with";

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct StatusItem {
    label: SharedString,
    tooltip: SharedString,
    scope: Option<ToolchainScope>,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn refresh_toolchain_status(&mut self, document: &Document) {
        let families = self.language_families(&document.path);
        if families.is_empty() {
            self.toolchains.statuses.remove(document);
            return;
        }
        let resolving = self.toolchain_resolving(document);
        let items = families
            .iter()
            .map(|family| {
                let scope = self.toolchains.scope_of(document, &family.id).cloned();
                let effective = scope
                    .as_ref()
                    .and_then(|scope| self.toolchains.effective(scope));
                let label = status_label(family, effective.as_ref(), resolving);
                let tooltip = effective
                    .as_ref()
                    .and_then(|effective| effective.toolchain())
                    .and_then(|toolchain| toolchain.path())
                    .map_or_else(
                        || CHOOSE_HINT.to_owned(),
                        |path| format!("{}\n{CHOOSE_HINT}", path.display()),
                    );
                StatusItem {
                    label: label.into(),
                    tooltip: tooltip.into(),
                    scope,
                }
            })
            .collect();
        self.toolchains.statuses.insert(document.clone(), items);
    }

    pub(in crate::environment_shell) fn refresh_toolchain_statuses(&mut self) {
        let documents: Vec<Document> = self.toolchains.statuses.keys().cloned().collect();
        for document in documents {
            self.refresh_toolchain_status(&document);
        }
    }

    pub(in crate::environment_shell) fn toolchain_status(
        &self,
        live: &LiveEnv,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let document = Document::new(&live.data.host, path);
        let progress = self.language_servers.progress_label(&document);
        let statuses = self.toolchains.statuses.get(&document);
        if progress.is_none() && statuses.is_none() {
            return None;
        }
        let pal = palette(cx);
        let progress = progress.map(|label| {
            div()
                .px(px(SPACE_1))
                .py(px(SPACE_0_5))
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(label)
                .into_any_element()
        });
        let items: Vec<AnyElement> = statuses
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, status)| {
                let tooltip = status.tooltip.clone();
                let item = div()
                    .id((STATUS_ITEM_ID, index))
                    .px(px(SPACE_1))
                    .py(px(SPACE_0_5))
                    .rounded(px(RADIUS_XS))
                    .cursor_pointer()
                    .hover(|style| style.bg(pal.bg_hover))
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(status.label.clone())
                    .tooltip(move |_, cx| tooltip_view(tooltip.clone(), cx));
                match status.scope.clone() {
                    Some(scope) => item
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                                let origin = (event.position.x.as_f32(), event.position.y.as_f32());
                                this.open_toolchain_picker(
                                    &scope,
                                    origin,
                                    MenuPlacement::Above,
                                    cx,
                                );
                            }),
                        )
                        .into_any_element(),
                    None => item.into_any_element(),
                }
            })
            .collect();
        Some(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .justify_end()
                .items_center()
                .gap(px(SPACE_2))
                .px(px(SPACE_2))
                .py(px(SPACE_0_5))
                .border_t_1()
                .border_color(pal.divider)
                .children(progress)
                .children(items)
                .into_any_element(),
        )
    }
}
