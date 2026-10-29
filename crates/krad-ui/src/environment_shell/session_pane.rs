mod panes;

use panes::{pane_px, PANE_STRIP_H};
pub(super) use panes::{DraggedTab, PaneDrag};

use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, ClickEvent, Context, FontWeight, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, ParentElement, Pixels, Point,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{AgentPreset, FileView, PaneId, SessionPanes, TerminalId, UnitRect};
use krad_session::SessionState;

use super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::sessions_view::session_terminals;
use super::terminal_body::{
    body_grid, BodyGeometry, BodyLook, BODY_PAD_TOP, BODY_PAD_X, TERMINAL_FONT_PX, TERMINAL_LINE_PX,
};
use super::terminal_card::error_line;
use super::terminal_status::CardStatus;
use super::{EnvironmentShell, LiveEnv};
use crate::components::{
    agent_status, card, harness_icon, icon, tooltip_view, Button, ButtonVariant, CenteredEmpty,
    EmptyState, IconName,
};
use crate::terminal_theme::terminal_colors;
use crate::theme::{
    palette, row_fill, row_shadow, terminal_font_family, Palette, ICON_SM, LINE_SM, RADIUS_LG,
    RADIUS_SM, RADIUS_XS, SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM,
};

const TAB_H: f32 = 28.0;
const TAB_LABEL_MAX_W: f32 = 150.0;
const TAB_DIVIDER_W: f32 = 1.0;
const TAB_DIVIDER_H: f32 = 14.0;
const CLOSE_BUTTON: f32 = 18.0;
const STATUS_ICON: f32 = 12.0;
const DIRTY_DOT: f32 = 6.0;
const UNZOOMED: f32 = 1.0;
const TERMINAL_TAB_PREFIX: &str = "session-tab";
const TAB_CLOSE_PREFIX: &str = "session-tab-close";
const BTN_ADD_TAB: &str = "session-add-tab";
const BTN_ALL_TABS: &str = "session-all-tabs";
const ADD_TAB_TOOLTIP: &str = "New chat (right-click for a terminal)";
const ALL_TABS_TOOLTIP: &str = "All tabs";
const CLOSE_TAB_TOOLTIP: &str = "Close tab";
const REMOVE_LEAD_LABEL: &str = "Remove lead";
const BTN_NEW_SESSION: &str = "session-pane-new-session";
const NEW_SESSION_LABEL: &str = "New session";
const BTN_NEW_GROUP: &str = "session-pane-new-group";
const NEW_GROUP_LABEL: &str = "Choose a folder…";
const NO_GROUP_TITLE: &str = "Create a group first";
const NO_GROUP_TEXT: &str = "Sessions live in a group: pick the folder its agents work in.";
const NO_SESSION_TITLE: &str = "No session yet";
const NO_SESSION_TEXT: &str =
    "A group starts empty. Open a session to work in it with shells, agents and a lead.";
const EMPTY_TITLE: &str = "No tabs yet";
const EMPTY_TEXT: &str = "Open a shell or an agent with the + button.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum StripPlace {
    TopBar,
    Pane,
}

fn shown_tab(
    terminals: &[TerminalId],
    focused: Option<TerminalId>,
    last: Option<TerminalId>,
) -> Option<TerminalId> {
    focused
        .filter(|id| terminals.contains(id))
        .or_else(|| last.filter(|id| terminals.contains(id)))
        .or_else(|| terminals.first().copied())
}

impl LiveEnv {
    fn session_tabs(&self) -> Vec<TerminalId> {
        let Some(session) = self.data.current_session() else {
            return Vec::new();
        };
        let lead = session.lead.as_ref().and_then(|lead| lead.terminal);
        lead.into_iter()
            .chain(session_terminals(session).filter(|id| Some(*id) != lead))
            .collect()
    }

    pub(super) fn shown_tab(&self) -> Option<TerminalId> {
        let last = self
            .data
            .current_session()
            .and_then(|session| session.tabs.active_tab())
            .and_then(|tab| tab.tiles.first())
            .map(|tile| tile.id);
        shown_tab(&self.session_tabs(), self.focused, last)
    }
}

pub(super) fn tab_shell(
    id: impl Into<gpui_kit::ElementId>,
    shown: bool,
    pal: &Palette,
    cx: &gpui_kit::App,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(SPACE_1_5))
        .h(px(TAB_H))
        .pl(px(SPACE_2))
        .pr(px(if shown { SPACE_1 } else { SPACE_2 }))
        .rounded(px(RADIUS_SM))
        .cursor_pointer()
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(if shown {
            pal.text_strong
        } else {
            pal.text_muted
        }))
        .when(shown, |tab| {
            tab.font_weight(FontWeight::MEDIUM)
                .bg(row_fill(pal, cx))
                .shadow(row_shadow(pal, cx))
        })
        .when(!shown, |tab| {
            tab.hover(|style| style.bg(pal.bg_hover).text_color(rgb(pal.text_default)))
        })
}

fn tab_divider(pal: &Palette) -> AnyElement {
    div()
        .flex_none()
        .w(px(TAB_DIVIDER_W))
        .h(px(TAB_DIVIDER_H))
        .mx(px(SPACE_0_5))
        .bg(pal.divider)
        .into_any_element()
}

fn divides(index: usize, shown: Option<usize>) -> bool {
    shown.is_none_or(|shown| shown != index && shown != index + 1)
}

fn position_of(event: &ClickEvent) -> (f32, f32) {
    let Point { x, y }: Point<Pixels> = event.position();
    (x.as_f32(), y.as_f32())
}

impl EnvironmentShell {
    pub(super) fn session_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Some(live) = self.envs.get(&self.active) else {
            return div().into_any_element();
        };
        if live.data.groups.is_empty() {
            let pick = Button::new(BTN_NEW_GROUP)
                .label(NEW_GROUP_LABEL)
                .icon(IconName::FolderAdd)
                .variant(ButtonVariant::Primary)
                .on_click(cx.listener(|_: &mut Self, _, _, cx| Self::prompt_new_group(cx)));
            return card(pal, cx)
                .size_full()
                .child(CenteredEmpty::new(NO_GROUP_TITLE, NO_GROUP_TEXT).action(pick))
                .into_any_element();
        }
        if live.data.current_session().is_none() {
            let group = live.selected_group;
            let open = Button::new(BTN_NEW_SESSION)
                .label(NEW_SESSION_LABEL)
                .icon(IconName::Add)
                .variant(ButtonVariant::Primary)
                .on_click(cx.listener(move |this: &mut Self, _, _window, cx| {
                    this.start_chat_session(group, cx);
                }));
            return card(pal, cx)
                .size_full()
                .child(CenteredEmpty::new(NO_SESSION_TITLE, NO_SESSION_TEXT).action(open))
                .into_any_element();
        }
        if live.session_tabs().is_empty() {
            return card(pal, cx)
                .size_full()
                .child(EmptyState::new(IconName::Layers, EMPTY_TITLE, EMPTY_TEXT))
                .into_any_element();
        }
        self.session_panes_view(live, cx)
    }

    pub(super) fn top_bar_tabs(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let live = self.envs.get(&self.active)?;
        let session = live.data.current_session()?;
        let [pane] = session.panes.panes.as_slice() else {
            return None;
        };
        let id = pane.id;
        Some(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_w_0()
                .h_full()
                .occlude()
                .on_drop(
                    cx.listener(move |this: &mut Self, dragged: &DraggedTab, _, cx| {
                        let tab = dragged.id;
                        this.edit_panes(cx, |panes| {
                            panes.move_tab(tab, id, usize::MAX).unwrap_or_default();
                        });
                    }),
                )
                .child(self.pane_strip(live, &session.panes, id, StripPlace::TopBar, cx))
                .into_any_element(),
        )
    }

    fn pane_strip(
        &self,
        live: &LiveEnv,
        panes: &SessionPanes,
        pane: PaneId,
        place: StripPlace,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = panes.pane(pane).and_then(|pane| pane.shown);
        let pal = palette(cx);
        let ids = panes.pane(pane).map_or_default(|pane| pane.tabs.clone());
        let shown_index = shown.and_then(|shown| ids.iter().position(|id| *id == shown));
        let mut tabs: Vec<AnyElement> = Vec::new();
        for (index, id) in ids.iter().copied().enumerate() {
            if index > 0 && divides(index - 1, shown_index) {
                tabs.push(tab_divider(pal));
            }
            let tab = self
                .terminal_tab(live, id, shown == Some(id), cx)
                .or_else(|| Self::guest_tab(live, id, shown == Some(id), cx));
            tabs.extend(tab);
        }
        let listed: Vec<MenuEntry> = ids
            .iter()
            .filter_map(|id| {
                let node = live.data.nodes.iter().find(|node| node.id == *id)?;
                Some(MenuEntry::checked(
                    self.title_label(*id, &node.title),
                    shown == Some(*id),
                    MenuAction::ShowSessionTab(*id),
                ))
            })
            .collect();
        let listed = std::rc::Rc::new(listed);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_0_5))
            .map(|strip| match place {
                StripPlace::TopBar => strip.flex_1().min_w_0().h_full(),
                StripPlace::Pane => strip
                    .flex_none()
                    .h(px(PANE_STRIP_H))
                    .px(px(SPACE_1_5))
                    .border_b_1()
                    .border_color(pal.divider),
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .children(tabs),
            )
            .child(
                div()
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this: &mut Self, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.open_harness_menu(event.position, MenuPlacement::Below, cx);
                        }),
                    )
                    .child(
                        Button::new((BTN_ADD_TAB, u64::from(pane.0)))
                            .icon(IconName::Add)
                            .small()
                            .tooltip(ADD_TAB_TOOLTIP)
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.pane_target = Some(pane);
                                this.add_chat_tab(cx);
                            })),
                    ),
            )
            .child(
                Button::new((BTN_ALL_TABS, u64::from(pane.0)))
                    .icon(IconName::ChevronDown)
                    .small()
                    .tooltip(ALL_TABS_TOOLTIP)
                    .on_click(
                        cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                            this.open_menu(
                                position_of(event),
                                MenuPlacement::Below,
                                listed.to_vec(),
                                cx,
                            );
                        }),
                    ),
            )
            .into_any_element()
    }

    fn terminal_tab(
        &self,
        live: &LiveEnv,
        id: TerminalId,
        shown: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pal = palette(cx);
        let node = live.data.nodes.iter().find(|node| node.id == id)?;
        let session = live.sessions.get(&id);
        let preset = session
            .and_then(|session| session.harness)
            .or(node.harness)
            .unwrap_or(AgentPreset::Shell);
        let leads = live.data.session_led_by(id).is_some();
        let file = node.file.as_ref();
        let preview = file.is_some_and(|file| !file.pinned);
        let dirty = live
            .editors
            .get(&id)
            .is_some_and(super::file_tab::FileEditor::is_dirty);
        let (mark, mark_color) = if leads {
            (IconName::Crown, pal.text_default)
        } else if file.is_some() {
            (IconName::File, pal.text_muted)
        } else {
            harness_icon(preset, pal)
        };
        let status = CardStatus::of(node.state, session);
        let on_chat = live.chats.contains_key(&id);
        let is_file = file.is_some();
        let leading = if status.is_quiet() {
            icon(mark, ICON_SM, mark_color)
        } else {
            agent_status(status.kind(), ICON_SM, pal)
        };
        let title = self.title_label(id, &node.title);
        let dragged = DraggedTab::new(id, title.clone(), mark, mark_color, cx.entity().downgrade());
        Some(
            tab_shell((TERMINAL_TAB_PREFIX, id.as_u64()), shown, pal, cx)
                .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                .on_mouse_up(
                    MouseButton::Middle,
                    cx.listener(move |this: &mut Self, _: &MouseUpEvent, window, cx| {
                        this.request_close_tab(id, window, cx);
                    }),
                )
                .on_click(
                    cx.listener(move |this: &mut Self, event: &ClickEvent, window, cx| {
                        if is_file {
                            this.show_file_tab(id, cx);
                        } else if shown {
                            this.open_terminal_tab_menu(id, position_of(event), cx);
                        } else {
                            this.show_terminal_tab(id, window, cx);
                        }
                    }),
                )
                .when(preview, |tab| {
                    tab.on_double_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.pin_file_tab(id, cx);
                    }))
                })
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let position = (event.position.x.as_f32(), event.position.y.as_f32());
                        this.open_terminal_tab_menu(id, position, cx);
                    }),
                )
                .child(leading)
                .child(
                    div()
                        .max_w(px(TAB_LABEL_MAX_W))
                        .truncate()
                        .when(preview, Styled::italic)
                        .child(title),
                )
                .when(dirty, |tab| {
                    tab.child(
                        div()
                            .flex_none()
                            .size(px(DIRTY_DOT))
                            .rounded_full()
                            .bg(rgb(pal.text_default)),
                    )
                })
                .when(on_chat, |tab| {
                    tab.child(icon(IconName::Chat, STATUS_ICON, pal.text_muted))
                })
                .when(shown, |tab| tab.child(Self::close_tab_button(id, cx)))
                .into_any_element(),
        )
    }

    fn close_tab_button(id: TerminalId, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .id((TAB_CLOSE_PREFIX, id.as_u64()))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(CLOSE_BUTTON))
            .rounded(px(RADIUS_XS))
            .hover(|style| style.bg(pal.bg_hover))
            .tooltip(|_, cx| tooltip_view(CLOSE_TAB_TOOLTIP.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                cx.stop_propagation();
                this.request_close_tab(id, window, cx);
            }))
            .child(icon(IconName::Close, STATUS_ICON, pal.text_muted))
            .into_any_element()
    }

    fn pane_body(
        &self,
        live: &LiveEnv,
        id: TerminalId,
        rect: UnitRect,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(chat) = live.chats.get(&id) {
            return self.surface_chat_body(id, chat, cx);
        }
        let node = live.data.nodes.iter().find(|node| node.id == id);
        if node.is_none() && !live.data.side_panel.holds(id) {
            return div().into_any_element();
        }
        match node
            .and_then(|node| node.file.as_ref())
            .map(|file| file.view)
        {
            Some(FileView::Edit) => return Self::file_tab_body(live, id, cx),
            Some(FileView::Changes) => return self.changes_tab_body(live, id, cx),
            None => {}
        }
        let pal = palette(cx);
        let session = live.sessions.get(&id);
        let mono = gpui_kit::SharedString::from(terminal_font_family(self.settings.terminal_font));
        let colors = terminal_colors(self.settings.terminal_theme, pal);
        let text_zoom = node.map_or(UNZOOMED, |node| node.text_scale.as_f32());
        let (pane_x, pane_y, _, _) = pane_px(rect, self.window_origin(), self.canvas_px);
        let shift = self.activity_shift();
        let geometry = BodyGeometry {
            origin: (
                pane_x + BODY_PAD_X,
                pane_y + self.pane_strip_h() + BODY_PAD_TOP + shift,
            ),
            cell_w: self.cell_width_px.unwrap_or(TERMINAL_FONT_PX) * text_zoom,
            line_h: TERMINAL_LINE_PX * text_zoom,
        };
        let failure = match session.map(|session| &session.state) {
            Some(SessionState::Failed(text)) => Some(error_line(text, pal)),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut Self, _: &MouseDownEvent, window, cx| {
                    this.focus_pane(window, cx);
                    this.focus_terminal(id);
                    cx.notify();
                }),
            )
            .children(failure)
            .child(div().flex_none().mt(px(shift)))
            .child(Self::terminal_body(
                id,
                session,
                self.terminal_images(id),
                BodyLook {
                    geometry,
                    ligatures: self.settings.ligatures,
                    zoom: UNZOOMED,
                    text_zoom,
                    focused: true,
                    corner: RADIUS_LG,
                },
                &mono,
                &colors,
                cx,
            ))
            .into_any_element()
    }

    fn open_terminal_tab_menu(
        &mut self,
        id: TerminalId,
        position: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let mut entries = self.surface_menu(id);
        let led = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.session_led_by(id));
        if let Some(session) = led {
            entries.push(MenuEntry::item(
                REMOVE_LEAD_LABEL,
                Some(IconName::Crown),
                MenuAction::DismissLead(session),
            ));
            entries.push(MenuEntry::Separator);
        }
        entries.extend(Self::terminal_menu(id));
        self.open_menu(position, MenuPlacement::Below, entries, cx);
    }

    pub(super) fn switch_surface(
        &mut self,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let live = self.envs.get(&self.active);
        let on_chat = live.is_some_and(|live| live.chats.contains_key(&id));
        let opencode = live
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .is_some_and(|node| node.harness == Some(AgentPreset::Opencode));
        if opencode && !on_chat {
            self.return_to_chat(id, cx);
        } else if on_chat {
            self.switch_to_terminal(id, cx);
        } else {
            self.switch_to_chat(id, window, cx);
        }
    }

    pub(super) fn show_terminal_tab(
        &mut self,
        id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_pane(window, cx);
        self.focus_terminal(id);
        cx.notify();
    }

    pub(super) fn sync_session_grid(&mut self, window: &mut Window) {
        let cell_width = self.cell_width(window);
        let size = self.canvas_px;
        let strip_h = self.pane_strip_h();
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let Some(session) = live.data.current_session() else {
            return;
        };
        let shown: Vec<(TerminalId, UnitRect)> = session
            .panes
            .layout()
            .into_iter()
            .filter_map(|(pane, rect)| Some((session.panes.pane(pane)?.shown?, rect)))
            .collect();
        for (id, rect) in shown {
            let (_, _, width, height) = pane_px(rect, (0.0, 0.0), size);
            let text_scale = live
                .data
                .nodes
                .iter()
                .find(|node| node.id == id)
                .map_or(UNZOOMED, |node| node.text_scale.as_f32());
            if let Some(session) = live.sessions.get_mut(&id) {
                session.resize_grid(body_grid(width, height, strip_h, cell_width, text_scale));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: TerminalId = TerminalId(1);
    const TWO: TerminalId = TerminalId(2);
    const ELSEWHERE: TerminalId = TerminalId(9);

    #[test]
    fn the_focused_terminal_of_the_session_is_shown() {
        assert_eq!(shown_tab(&[ONE, TWO], Some(TWO), Some(ONE)), Some(TWO));
    }

    #[test]
    fn a_terminal_of_another_session_falls_back_to_the_first_tab() {
        assert_eq!(shown_tab(&[ONE, TWO], Some(ELSEWHERE), None), Some(ONE));
        assert_eq!(shown_tab(&[], Some(ONE), None), None);
    }

    #[test]
    fn a_focused_system_terminal_keeps_the_tab_last_shown() {
        assert_eq!(
            shown_tab(&[ONE, TWO], Some(ELSEWHERE), Some(TWO)),
            Some(TWO)
        );
    }

    #[test]
    fn no_rule_touches_the_shown_tab() {
        assert!(!divides(0, Some(1)), "left of the shown tab");
        assert!(!divides(1, Some(1)), "right of the shown tab");
        assert!(divides(2, Some(1)));
        assert!(divides(0, None));
    }
}
