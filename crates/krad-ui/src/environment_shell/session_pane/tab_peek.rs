use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyView, App, AppContext as _, Context, FontWeight, IntoElement, ParentElement,
    Render, SharedString, Styled, WeakEntity, Window,
};
use krad_domain::{AgentPreset, TerminalId};

use super::super::EnvironmentShell;
use crate::components::{harness_icon, icon, solid_card, IconName};
use crate::theme::{
    palette, ICON_SM, LINE_SM, LINE_XS, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const PEEK_MAX_W: f32 = 320.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct TabPeek {
    pub(in crate::environment_shell) title: SharedString,
    pub(in crate::environment_shell) preset: AgentPreset,
    pub(in crate::environment_shell) activity: Option<SharedString>,
    pub(in crate::environment_shell) folder: Option<SharedString>,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn tab_peek(&self, id: TerminalId) -> Option<TabPeek> {
        let live = self.envs.get(&self.active)?;
        let node = live.data.nodes.iter().find(|node| node.id == id)?;
        let session = live.sessions.get(&id);
        let preset = session
            .and_then(|session| session.harness)
            .or(node.harness)?;
        let folder = session
            .and_then(|session| session.cwd.clone())
            .filter(|cwd| !cwd.is_empty())
            .or_else(|| {
                live.data
                    .groups
                    .iter()
                    .find(|group| node.group == Some(group.id))
                    .map(|group| group.root.display().to_string())
            });
        Some(TabPeek {
            title: self.title_label(id, &node.title).into(),
            preset,
            activity: session
                .and_then(|session| session.activity.clone())
                .map(SharedString::from),
            folder: folder.map(SharedString::from),
        })
    }
}

pub(in crate::environment_shell) fn tab_peek_view(
    shell: &WeakEntity<EnvironmentShell>,
    id: TerminalId,
    cx: &mut App,
) -> Option<AnyView> {
    let peek = shell.upgrade()?.read(cx).tab_peek(id)?;
    Some(cx.new(|_| peek).into())
}

impl Render for TabPeek {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = palette(cx);
        let (mark, mark_color) = harness_icon(self.preset, pal);
        solid_card(pal)
            .max_w(px(PEEK_MAX_W))
            .gap(px(SPACE_1))
            .px(px(SPACE_3))
            .py(px(SPACE_2))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(icon(mark, ICON_SM, mark_color))
                    .child(div().min_w_0().truncate().child(self.title.clone())),
            )
            .when_some(self.activity.clone(), |peek, activity| {
                peek.child(div().text_color(rgb(pal.text_default)).child(activity))
            })
            .when_some(self.folder.clone(), |peek, folder| {
                peek.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_1_5))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(icon(IconName::Folder, ICON_SM, pal.text_muted))
                        .child(div().min_w_0().truncate().child(folder)),
                )
            })
    }
}
