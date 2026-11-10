use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{
    AttentionTarget, EnvironmentName, EnvironmentView, KraddieSessionLink, SessionId,
};

use super::super::super::EnvironmentShell;
use super::super::motion::entering;
use super::super::KraddieEntry;
use crate::components::{icon, IconName};
use crate::kraddie_avatar::kraddie_avatar;
use crate::theme::{
    palette, row_fill, row_shadow, Palette, ICON_XS, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS,
    SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2, SPACE_5, TEXT_SM, TEXT_XS,
};

const AVATAR_SIDE: f32 = 20.0;
const SECTION_TITLE: &str = "Kraddies";
const ROW_PREFIX: &str = "sidebar-kraddie";
const SESSION_PREFIX: &str = "sidebar-kraddie-session";
const ENTER_PREFIX: &str = "sidebar-kraddie-enter";
const PRESSED_OPACITY: f32 = 0.7;

fn live_links(entry: &KraddieEntry) -> impl Iterator<Item = &KraddieSessionLink> {
    entry
        .runs_newest_first()
        .into_iter()
        .filter(|run| run.outcome.is_live())
        .filter_map(|run| run.session.as_ref())
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn kraddie_session_hidden(
        &self,
        env: &EnvironmentName,
        session: SessionId,
    ) -> bool {
        self.kraddies
            .run_on(env, session)
            .is_some_and(|(entry, _)| !entry.kraddie.settings.shows_in_sidebar())
    }

    pub(in crate::environment_shell) fn sidebar_kraddies(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.kraddies.entries.is_empty() {
            return None;
        }
        let pal = palette(cx);
        let rows = self
            .kraddies
            .entries
            .iter()
            .enumerate()
            .flat_map(|(index, entry)| {
                let row = self.sidebar_kraddie_row(index, entry, pal, cx);
                let folder: Vec<AnyElement> = if entry.kraddie.settings.sessions.folder.is_on() {
                    self.kraddie_folder_rows(index, entry, pal, cx)
                } else {
                    Vec::new()
                };
                let chats = self.kraddie_chat_rows(&entry.kraddie.id(), cx);
                std::iter::once(row).chain(chats).chain(folder)
            })
            .collect::<Vec<_>>();
        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap(px(SPACE_0_5))
                .py(px(SPACE_1))
                .border_b_1()
                .border_color(pal.divider)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_1_5))
                        .px(px(SPACE_2))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(pal.text_subtle))
                        .child(icon(IconName::Kraddie, ICON_XS, pal.text_subtle))
                        .child(SECTION_TITLE),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    fn sidebar_kraddie_row(
        &self,
        index: usize,
        entry: &KraddieEntry,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = entry.kraddie.id();
        let mood = self.kraddie_mood(entry);
        let sprite = entry.sprite(mood);
        self.kraddies.note_drawn(sprite);
        let selected = self.active_view() == EnvironmentView::Kraddies
            && self.kraddies.profile.open.as_ref() == Some(&id);
        let enter_id = (SharedString::from(format!("{ENTER_PREFIX}-{id}")), index);
        let row = div()
            .id((ROW_PREFIX, index))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(NAV_ROW_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_strong))
            .when(selected, |row| {
                row.bg(row_fill(pal, cx))
                    .shadow(row_shadow(pal, cx))
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!selected, |row| row.hover(|style| style.bg(pal.bg_hover)))
            .active(|style| style.opacity(PRESSED_OPACITY))
            .child(self.kraddie_expander(index, &id, cx))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| this.open_kraddie_page(&id, cx)))
            .child(kraddie_avatar(sprite, AVATAR_SIDE, self.kraddies.frame()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(entry.kraddie.name.to_string()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(mood.label()),
            );
        entering(row, enter_id, index).into_any_element()
    }

    fn kraddie_folder_rows(
        &self,
        index: usize,
        entry: &KraddieEntry,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        live_links(entry)
            .filter_map(|link| {
                let live = self.envs.get(&link.environment)?;
                let name = live.data.session(link.session)?.name.clone();
                Some((link.clone(), name))
            })
            .enumerate()
            .map(|(row, (link, name))| {
                div()
                    .id(SharedString::from(format!(
                        "{SESSION_PREFIX}-{index}-{row}"
                    )))
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .h(px(NAV_ROW_H))
                    .pl(px(SPACE_5 + AVATAR_SIDE))
                    .pr(px(SPACE_2))
                    .rounded(px(RADIUS_XS))
                    .cursor_pointer()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_default))
                    .hover(|style| style.bg(pal.bg_hover))
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        let target = AttentionTarget::new(link.environment.clone(), link.terminal);
                        this.open_attention(&target, window, cx);
                    }))
                    .child(div().truncate().child(name))
                    .into_any_element()
            })
            .collect()
    }
}
