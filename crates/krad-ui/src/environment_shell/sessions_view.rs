use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{
    AgentState, EnvironmentView, GroupId, Session, SessionId, SessionSpec, TerminalId,
};

use super::context_menu::{MenuAction, MenuEntry};
use super::session_look::{session_icon, session_wash};
use super::terminal_status::CardStatus;
use super::{EnvironmentShell, LiveEnv};
use crate::components::{agent_status, icon, Button, IconName};
use crate::theme::{
    palette, row_fill, row_shadow, ICON_SM, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_1,
    SPACE_1_5, SPACE_2, TEXT_SM, TEXT_XS,
};

const SESSION_ROW_PREFIX: &str = "session-row";
const SESSION_CLOSE_PREFIX: &str = "close-session-";
const CLOSE_SESSION_TOOLTIP: &str = "Close session; its terminals move to another one";
const TABS_SUFFIX: &str = "tabs";
const CUSTOMIZE_LABEL: &str = "Customize…";
const CLOSE_SESSION_LABEL: &str = "Close session";
const SESSION_ROW_GROUP_PREFIX: &str = "session-row-hover-";
const SESSION_ROW_PAD_LEFT: f32 = SPACE_1;

pub(super) fn session_terminals(session: &Session) -> impl Iterator<Item = TerminalId> + '_ {
    session
        .tabs
        .tabs
        .iter()
        .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
}

const fn urgency(status: CardStatus) -> u8 {
    match status {
        CardStatus::Agent(AgentState::WaitingForInput) => 5,
        CardStatus::Failed => 4,
        CardStatus::Agent(AgentState::Working) => 3,
        CardStatus::Agent(AgentState::Done) => 2,
        CardStatus::Exited => 1,
        CardStatus::Agent(AgentState::Idle) => 0,
    }
}

fn session_status(statuses: impl IntoIterator<Item = CardStatus>) -> CardStatus {
    statuses
        .into_iter()
        .max_by_key(|status| urgency(*status))
        .unwrap_or(CardStatus::Agent(AgentState::Idle))
}

impl EnvironmentShell {
    pub(super) fn session_rows(
        &self,
        group: Option<GroupId>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(live) = self.envs.get(&self.active) else {
            return Vec::new();
        };
        live.data
            .sessions_of(group)
            .map(|session| {
                let row = SessionRow {
                    session,
                    status: Self::status_of(live, session),
                    active: live.data.active_session == Some(session.id),
                    closable: live.data.can_close_session(session.id),
                };
                Self::session_row(&row, cx)
            })
            .collect()
    }

    fn session_menu(id: SessionId, closable: bool) -> Vec<MenuEntry> {
        let mut entries = vec![MenuEntry::item(
            CUSTOMIZE_LABEL,
            Some(IconName::Magic),
            MenuAction::CustomizeSession(id),
        )];
        if closable {
            entries.extend([
                MenuEntry::Separator,
                MenuEntry::destructive(
                    CLOSE_SESSION_LABEL,
                    IconName::Close,
                    MenuAction::CloseSession(id),
                ),
            ]);
        }
        entries
    }

    fn status_of(live: &LiveEnv, session: &Session) -> CardStatus {
        session_status(session_terminals(session).filter_map(|id| {
            let node = live.data.nodes.iter().find(|node| node.id == id)?;
            Some(CardStatus::of(node.state, live.sessions.get(&id)))
        }))
    }

    fn session_row(row: &SessionRow<'_>, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let id = row.session.id;
        let raw = id.as_u64();
        let unread = row.status == CardStatus::Agent(AgentState::WaitingForInput);
        let count = session_terminals(row.session).count();
        let closable = row.closable;
        let look = row.session.look;
        let wash = look.color.map(|hue| session_wash(hue, row.active, pal));
        let glyph = look.icon.map_or(IconName::Layers, session_icon);
        let quiet = matches!(
            row.status,
            CardStatus::Agent(AgentState::Idle) | CardStatus::Exited
        );
        let hover_group = SharedString::from(format!("{SESSION_ROW_GROUP_PREFIX}{raw}"));
        let reveal = hover_group.clone();
        div()
            .id((SESSION_ROW_PREFIX, raw))
            .group(hover_group)
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(NAV_ROW_H))
            .pl(px(SESSION_ROW_PAD_LEFT))
            .pr(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .font_weight(if unread {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .text_color(rgb(pal.text_strong))
            .when(row.active, |row| {
                row.bg(row_fill(pal, cx)).shadow(row_shadow(pal, cx))
            })
            .when_some(wash, Styled::bg)
            .hover(|style| style.bg(pal.bg_hover))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.select_session(id, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                    this.open_menu_at(event, Self::session_menu(id, closable), cx);
                }),
            )
            .child(icon(glyph, ICON_SM, pal.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(row.session.name.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .invisible()
                    .group_hover(reveal, Styled::visible)
                    .child(
                        div()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(rgb(pal.text_muted))
                            .child(format!("{count} {TABS_SUFFIX}")),
                    )
                    .when(row.closable && row.active, |element| {
                        element.child(
                            Button::new(SharedString::from(format!("{SESSION_CLOSE_PREFIX}{raw}")))
                                .icon(IconName::Close)
                                .small()
                                .tooltip(CLOSE_SESSION_TOOLTIP)
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    cx.stop_propagation();
                                    this.close_session(id, cx);
                                })),
                        )
                    }),
            )
            .when(!quiet, |element| {
                element.child(agent_status(row.status.kind(), ICON_SM, pal))
            })
            .into_any_element()
    }

    pub(in crate::environment_shell) fn set_view(
        &mut self,
        view: EnvironmentView,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.data.view = view;
        }
        cx.notify();
        self.store();
    }

    fn with_sessions(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut LiveEnv)) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            edit(live);
            show_current_session(live);
        }
        cx.notify();
        self.store();
    }

    fn select_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        self.with_sessions(cx, |live| {
            live.data.select_session(id);
        });
    }

    pub(super) fn new_session(
        &mut self,
        group: Option<GroupId>,
        spec: SessionSpec,
        cx: &mut Context<Self>,
    ) {
        self.with_sessions(cx, |live| {
            live.data.new_session(group, spec);
        });
    }

    pub(super) fn close_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        self.with_sessions(cx, |live| {
            live.data.close_session(id);
        });
    }
}

struct SessionRow<'a> {
    session: &'a Session,
    status: CardStatus,
    active: bool,
    closable: bool,
}

fn show_current_session(live: &mut LiveEnv) {
    let Some(session) = live.data.current_session() else {
        return;
    };
    let current = session.id;
    live.selected_group = session.group;
    let shown = live
        .focused
        .is_some_and(|focused| live.data.session_of(focused) == Some(current));
    if !shown {
        live.focused = live
            .data
            .tabs()
            .and_then(|tabs| tabs.active_tab())
            .and_then(|tab| tab.tiles.first())
            .map(|tile| tile.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_shows_its_most_pressing_terminal() {
        let idle = CardStatus::Agent(AgentState::Idle);
        let working = CardStatus::Agent(AgentState::Working);
        let waiting = CardStatus::Agent(AgentState::WaitingForInput);
        let done = CardStatus::Agent(AgentState::Done);
        assert_eq!(session_status([idle, working, waiting]), waiting);
        assert_eq!(
            session_status([done, CardStatus::Failed, working]),
            CardStatus::Failed
        );
        assert_eq!(session_status([idle, done]), done);
        assert_eq!(session_status([]), idle);
    }
}
