use gpui_kit::component::input::Input;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, Window,
};
use krad_domain::{ArchivedSession, ArchivedSessionId, LooseChat, TerminalId};
use std::time::SystemTime;

use super::super::dialog_parts::{
    sheet_body, sheet_group, sheet_overlay, sheet_padded, sheet_panel, sheet_row_with,
    sheet_section, sheet_title,
};
use super::super::session_look::session_icon;
use super::super::surface::age_label;
use super::super::EnvironmentShell;
use super::listing::{listing, Listing};
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{hue_rgb, palette, Palette, ICON_SM, LINE_SM, SPACE_1_5, SPACE_6, TEXT_SM};

const SHEET_W: f32 = 600.0;
const TITLE: &str = "Archive";
const SESSIONS_SECTION: &str = "Closed sessions";
const OPEN_SECTION_PREFIX: &str = "Closed from ";
const GONE_SECTION: &str = "Closed chats";
const EMPTY_TEXT: &str = "Closed sessions and chats of this group show here.";
const NO_MATCH_TEXT: &str = "Nothing in the Archive matches.";
const ONE_CHAT: &str = "1 chat";
const CHATS_SUFFIX: &str = "chats";
const CLOSED_PREFIX: &str = "closed";
const DETAIL_SEPARATOR: &str = " · ";
const REOPEN_LABEL: &str = "Reopen";
const DELETE_TOOLTIP: &str = "Delete for good";
const BTN_CLOSE: &str = "archive-close";
const BODY_ID: &str = "archive-body";
const REOPEN_SESSION_PREFIX: &str = "archive-reopen-session-";
const DELETE_SESSION_PREFIX: &str = "archive-delete-session-";
const REOPEN_CHAT_PREFIX: &str = "archive-reopen-chat-";
const DELETE_CHAT_PREFIX: &str = "archive-delete-chat-";
const LOOSE_INDENT: f32 = SPACE_6;

#[derive(Clone, Copy)]
enum Archived {
    Session(ArchivedSessionId),
    Chat(TerminalId),
}

impl Archived {
    fn ids(self) -> (String, String) {
        match self {
            Self::Session(id) => (
                format!("{REOPEN_SESSION_PREFIX}{}", id.as_u64()),
                format!("{DELETE_SESSION_PREFIX}{}", id.as_u64()),
            ),
            Self::Chat(id) => (
                format!("{REOPEN_CHAT_PREFIX}{}", id.as_u64()),
                format!("{DELETE_CHAT_PREFIX}{}", id.as_u64()),
            ),
        }
    }
}

fn close(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_archive(cx);
}

fn chats_label(count: usize) -> String {
    match count {
        1 => ONE_CHAT.to_owned(),
        count => format!("{count} {CHATS_SUFFIX}"),
    }
}

fn closed_label(closed: SystemTime, now: SystemTime) -> String {
    let elapsed = now.duration_since(closed).unwrap_or_default();
    format!("{CLOSED_PREFIX} {}", age_label(elapsed))
}

fn session_detail(session: &ArchivedSession, now: SystemTime) -> SharedString {
    [
        chats_label(session.chat_count()),
        closed_label(session.closed, now),
    ]
    .join(DETAIL_SEPARATOR)
    .into()
}

fn note(text: &'static str, pal: &Palette) -> AnyElement {
    div()
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(pal.text_subtle))
        .child(text)
        .into_any_element()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn archive_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let live = self.envs.get(&self.active)?;
        let sheet = live.archive_sheet.as_ref()?;
        let pal = palette(cx);
        let group = sheet.group;
        let query = sheet.search.read(cx).value().to_string();
        let group_name = live.data.group(group).map(|group| group.name.clone());
        let listed = listing(&live.data.archive, group, &query);
        let empty = if query.trim().is_empty() {
            EMPTY_TEXT
        } else {
            NO_MATCH_TEXT
        };
        let body = if listed.is_empty() {
            sheet_body(BODY_ID).child(note(empty, pal))
        } else {
            sheet_body(BODY_ID).children(Self::archive_sections(&listed, cx))
        };
        let panel = sheet_panel(SHEET_W, pal)
            .child(sheet_title(TITLE, group_name, BTN_CLOSE, close, cx))
            .child(sheet_padded(Input::new(&sheet.search)))
            .child(body);
        Some(sheet_overlay(panel, close, cx))
    }

    fn archive_sections(listed: &Listing<'_>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pal = palette(cx);
        let now = SystemTime::now();
        let mut sections = Vec::new();
        if !listed.sessions.is_empty() {
            let mut rows = Vec::new();
            for entry in &listed.sessions {
                rows.push(Self::archived_session_row(entry.session, now, cx));
                rows.extend(
                    entry
                        .chats
                        .iter()
                        .map(|loose| Self::loose_chat_row(loose, true, now, cx)),
                );
            }
            sections.push(
                sheet_section(SESSIONS_SECTION, None, pal)
                    .child(sheet_group(rows, pal))
                    .into_any_element(),
            );
        }
        for shelf in &listed.open {
            let rows: Vec<AnyElement> = shelf
                .chats
                .iter()
                .map(|loose| Self::loose_chat_row(loose, false, now, cx))
                .collect();
            sections.push(
                sheet_section(format!("{OPEN_SECTION_PREFIX}{}", shelf.name), None, pal)
                    .child(sheet_group(rows, pal))
                    .into_any_element(),
            );
        }
        if !listed.gone.is_empty() {
            let rows: Vec<AnyElement> = listed
                .gone
                .iter()
                .map(|loose| Self::loose_chat_row(loose, false, now, cx))
                .collect();
            sections.push(
                sheet_section(GONE_SECTION, None, pal)
                    .child(sheet_group(rows, pal))
                    .into_any_element(),
            );
        }
        sections
    }

    fn archived_session_row(
        session: &ArchivedSession,
        now: SystemTime,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let tone = session
            .look
            .color
            .map_or(pal.text_muted, |hue| hue_rgb(hue, pal));
        let glyph = session.look.icon.map_or(IconName::Layers, session_icon);
        sheet_row_with(
            Some(icon(glyph, ICON_SM, tone).into_any_element()),
            session.name.clone(),
            Some(session_detail(session, now)),
            Some(Self::archive_actions(Archived::Session(session.id), cx)),
            pal,
        )
        .into_any_element()
    }

    fn loose_chat_row(
        loose: &LooseChat,
        under_session: bool,
        now: SystemTime,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let row = sheet_row_with(
            Some(icon(IconName::Chat, ICON_SM, pal.text_subtle).into_any_element()),
            loose.chat.title.clone(),
            Some(closed_label(loose.closed, now).into()),
            Some(Self::archive_actions(
                Archived::Chat(loose.chat.terminal),
                cx,
            )),
            pal,
        );
        if under_session {
            return row.pl(px(LOOSE_INDENT)).into_any_element();
        }
        row.into_any_element()
    }

    fn archive_actions(archived: Archived, cx: &mut Context<Self>) -> AnyElement {
        let (reopen_id, delete_id) = archived.ids();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .child(
                Button::new(SharedString::from(reopen_id))
                    .label(REOPEN_LABEL)
                    .icon(IconName::Undo)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(
                        cx.listener(move |this: &mut Self, _, _, cx| match archived {
                            Archived::Session(id) => this.reopen_archived_session(id, cx),
                            Archived::Chat(id) => this.reopen_archived_chat(id, cx),
                        }),
                    ),
            )
            .child(
                Button::new(SharedString::from(delete_id))
                    .icon(IconName::Close)
                    .small()
                    .variant(ButtonVariant::Ghost)
                    .tooltip(DELETE_TOOLTIP)
                    .on_click(
                        cx.listener(move |this: &mut Self, _, _, cx| match archived {
                            Archived::Session(id) => this.delete_archived_session(id, cx),
                            Archived::Chat(id) => this.delete_archived_chat(id, cx),
                        }),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_row_says_how_many_chats_it_holds_and_when_it_closed() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_hours(100);
        assert_eq!(chats_label(1), "1 chat");
        assert_eq!(chats_label(3), "3 chats");
        assert_eq!(
            closed_label(now - Duration::from_hours(50), now),
            "closed 2 d ago"
        );
        assert_eq!(
            closed_label(now + Duration::from_hours(1), now),
            "closed just now"
        );
    }
}
