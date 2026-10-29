use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, FontWeight, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{SystemTerminal, TerminalId, Visibility};

use super::super::session_pane::{tab_shell, DraggedTab};
use super::super::terminal_body::{
    body_grid, BodyGeometry, BodyLook, BODY_PAD_TOP, BODY_PAD_X, TERMINAL_FONT_PX, TERMINAL_LINE_PX,
};
use super::super::EnvironmentShell;
use super::ActivityOpened;
use crate::components::{icon, Button, IconName};
use crate::terminal_theme::terminal_colors;
use crate::theme::{
    palette, row_fill, terminal_font_family, CARD_HEADER_H, ICON_SM, LINE_SM, NAV_ROW_H, RADIUS_LG,
    RADIUS_XS, SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM, WINDOW_INSET,
};

const LOG_TAB_ID: &str = "utilities-log-tab";
const TERMINALS_TAB_ID: &str = "utilities-terminals-tab";
const DOCKED_TAB_PREFIX: &str = "utilities-docked-tab";
const UNDOCK_PREFIX: &str = "utilities-undock";
const LIST_ROW_PREFIX: &str = "utilities-terminal-row";
const LIST_CLOSE_PREFIX: &str = "utilities-terminal-close";
const BTN_ADD_TERMINAL: &str = "utilities-add-terminal";
const LOG_LABEL: &str = "Activity";
const TERMINALS_LABEL: &str = "Terminal";
const UNDOCK_TOOLTIP: &str = "Take out of the utilities panel";
const ADD_TERMINAL_TOOLTIP: &str = "New terminal";
const CLOSE_TERMINAL_TOOLTIP: &str = "Close terminal";
const TAB_LABEL_MAX_W: f32 = 140.0;
pub(super) const TERMINAL_LIST_W: f32 = 180.0;
const RULE: f32 = 1.0;
const UNZOOMED: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) enum TerminalsTab {
    #[default]
    Hidden,
    Shown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) enum UtilityTab {
    #[default]
    Log,
    Terminals,
    Docked(TerminalId),
}

impl EnvironmentShell {
    fn utility_tab(&self) -> UtilityTab {
        let Some(live) = self.envs.get(&self.active) else {
            return UtilityTab::Log;
        };
        match live.utility_tab {
            UtilityTab::Docked(id) if !live.activity_docked.contains(&id) => UtilityTab::Log,
            UtilityTab::Terminals
                if live.terminals_tab == TerminalsTab::Hidden
                    || self.utility_terminals().is_empty() =>
            {
                UtilityTab::Log
            }
            tab => tab,
        }
    }

    fn set_utility_tab(&mut self, tab: UtilityTab, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.utility_tab = tab;
        }
        cx.notify();
    }

    fn show_utilities(&mut self, tab: UtilityTab, cx: &mut Context<Self>) {
        self.set_utility_tab(tab, cx);
        self.activity = Visibility::Visible;
        self.activity_opened = ActivityOpened::Deliberately;
    }

    pub(in crate::environment_shell) fn new_utility_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.terminals_tab = TerminalsTab::Shown;
        }
        match self.shown_system_terminal() {
            Some(id) => self.show_system_terminal(id, window, cx),
            None => self.add_system_terminal(window, cx),
        }
        self.show_utilities(UtilityTab::Terminals, cx);
    }

    fn add_utility_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_system_terminal(window, cx);
        self.show_utilities(UtilityTab::Terminals, cx);
    }

    fn utility_terminals(&self) -> Vec<SystemTerminal> {
        self.system_terminals_here()
    }

    pub(super) fn dock_in_activity(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        self.edit_panes(cx, |panes| panes.release(id));
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            if !live.activity_docked.contains(&id) {
                live.activity_docked.push(id);
            }
        }
        self.show_utilities(UtilityTab::Docked(id), cx);
    }

    fn undock_from_activity(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.activity_docked.retain(|docked| *docked != id);
        }
        cx.notify();
    }

    pub(super) fn activity_terminal(&self) -> Option<TerminalId> {
        match self.utility_tab() {
            UtilityTab::Log => None,
            UtilityTab::Terminals => self.shown_system_terminal(),
            UtilityTab::Docked(id) => Some(id),
        }
    }

    fn docked_title(&self, id: TerminalId) -> String {
        let Some(live) = self.envs.get(&self.active) else {
            return String::new();
        };
        live.data
            .nodes
            .iter()
            .find(|node| node.id == id)
            .map(|node| self.title_label(id, &node.title))
            .or_else(|| {
                live.data
                    .side_panel
                    .terminals
                    .iter()
                    .find(|terminal| terminal.id == id)
                    .map(|terminal| terminal.title.clone())
            })
            .unwrap_or_default()
    }

    pub(super) fn activity_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let shown = self.utility_tab();
        let docked = self
            .envs
            .get(&self.active)
            .map_or_default(|live| live.activity_docked.clone());
        let log = tab_shell(LOG_TAB_ID, shown == UtilityTab::Log, pal, cx)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.set_utility_tab(UtilityTab::Log, cx);
            }))
            .pr(px(SPACE_2))
            .child(icon(IconName::PanelBottom, ICON_SM, pal.text_muted))
            .child(div().font_weight(FontWeight::MEDIUM).child(LOG_LABEL))
            .into_any_element();
        let offered = self
            .envs
            .get(&self.active)
            .is_some_and(|live| live.terminals_tab == TerminalsTab::Shown);
        let terminals = (offered && !self.utility_terminals().is_empty()).then(|| {
            tab_shell(TERMINALS_TAB_ID, shown == UtilityTab::Terminals, pal, cx)
                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.set_utility_tab(UtilityTab::Terminals, cx);
                }))
                .pr(px(SPACE_2))
                .child(icon(IconName::Terminal, ICON_SM, pal.text_muted))
                .child(div().font_weight(FontWeight::MEDIUM).child(TERMINALS_LABEL))
                .into_any_element()
        });
        let tabs = docked.into_iter().map(|id| {
            let title = SharedString::from(self.docked_title(id));
            let dragged = DraggedTab::new(
                id,
                title.clone(),
                IconName::Terminal,
                pal.text_muted,
                cx.entity().downgrade(),
            );
            tab_shell(
                (DOCKED_TAB_PREFIX, id.as_u64()),
                shown == UtilityTab::Docked(id),
                pal,
                cx,
            )
            .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_utility_tab(UtilityTab::Docked(id), cx);
                this.focus_terminal(id);
            }))
            .child(icon(IconName::Terminal, ICON_SM, pal.text_muted))
            .child(div().max_w(px(TAB_LABEL_MAX_W)).truncate().child(title))
            .child(
                Button::new((UNDOCK_PREFIX, id.as_u64()))
                    .icon(IconName::Close)
                    .small()
                    .tooltip(UNDOCK_TOOLTIP)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.undock_from_activity(id, cx);
                    })),
            )
            .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_0_5))
            .min_w_0()
            .overflow_hidden()
            .child(log)
            .children(terminals)
            .children(tabs)
            .into_any_element()
    }

    pub(super) fn utility_terminal_view(
        &self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let listed = self.utility_tab() == UtilityTab::Terminals;
        let terminals = self.utility_terminals();
        let list = (listed && terminals.len() > 1)
            .then(|| Self::utility_terminal_list(&terminals, id, cx));
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .child(self.docked_body(id, cx))
            .children(list)
            .into_any_element()
    }

    fn utility_terminal_list(
        terminals: &[SystemTerminal],
        shown: TerminalId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let rows = terminals.iter().map(|terminal| {
            let id = terminal.id;
            let selected = id == shown;
            div()
                .id((LIST_ROW_PREFIX, id.as_u64()))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_1_5))
                .h(px(NAV_ROW_H))
                .px(px(SPACE_2))
                .rounded(px(RADIUS_XS))
                .cursor_pointer()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(if selected {
                    pal.text_strong
                } else {
                    pal.text_default
                }))
                .when(selected, |row| row.bg(row_fill(pal, cx)))
                .when(!selected, |row| row.hover(|style| style.bg(pal.bg_hover)))
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.show_system_terminal(id, window, cx);
                }))
                .child(icon(IconName::Terminal, ICON_SM, pal.text_muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(terminal.title.clone()),
                )
                .child(
                    Button::new((LIST_CLOSE_PREFIX, id.as_u64()))
                        .icon(IconName::Close)
                        .small()
                        .tooltip(CLOSE_TERMINAL_TOOLTIP)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            cx.stop_propagation();
                            this.close_system_terminal(id, cx);
                        })),
                )
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(TERMINAL_LIST_W))
            .h_full()
            .p(px(SPACE_1))
            .gap(px(SPACE_0_5))
            .border_l(px(RULE))
            .border_color(pal.divider)
            .children(rows)
            .into_any_element()
    }

    pub(super) fn add_terminal_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        (self.utility_tab() == UtilityTab::Terminals).then(|| {
            Button::new(BTN_ADD_TERMINAL)
                .icon(IconName::Add)
                .small()
                .tooltip(ADD_TERMINAL_TOOLTIP)
                .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                    this.add_utility_terminal(window, cx);
                }))
                .into_any_element()
        })
    }

    fn docked_body(&self, id: TerminalId, cx: &mut Context<Self>) -> AnyElement {
        let Some(live) = self.envs.get(&self.active) else {
            return div().into_any_element();
        };
        let pal = palette(cx);
        let session = live.sessions.get(&id);
        let mono = SharedString::from(terminal_font_family(self.settings.terminal_font));
        let colors = terminal_colors(self.settings.terminal_theme, pal);
        let (origin_x, origin_y) = self.window_origin();
        let top = origin_y + self.canvas_px.1 + WINDOW_INSET;
        let geometry = BodyGeometry {
            origin: (origin_x + BODY_PAD_X, top + CARD_HEADER_H + BODY_PAD_TOP),
            cell_w: self.cell_width_px.unwrap_or(TERMINAL_FONT_PX),
            line_h: TERMINAL_LINE_PX,
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut Self, _: &MouseDownEvent, window, cx| {
                    this.focus_pane(window, cx);
                    this.focus_terminal(id);
                    cx.notify();
                }),
            )
            .child(Self::terminal_body(
                id,
                session,
                self.terminal_images(id),
                BodyLook {
                    geometry,
                    ligatures: self.settings.ligatures,
                    zoom: UNZOOMED,
                    text_zoom: UNZOOMED,
                    focused: live.focused == Some(id),
                    corner: RADIUS_LG,
                },
                &mono,
                &colors,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::environment_shell) fn sync_activity_grid(&mut self, window: &mut Window) {
        if !self.activity.is_visible() {
            return;
        }
        if self.utility_tab() == UtilityTab::Terminals {
            self.start_system_terminals();
        }
        let Some(id) = self.activity_terminal() else {
            return;
        };
        let listed =
            self.utility_tab() == UtilityTab::Terminals && self.utility_terminals().len() > 1;
        let cell_width = self.cell_width(window);
        let width = self.content_row_width(window) - if listed { TERMINAL_LIST_W } else { 0.0 };
        let height = self.activity_log.height;
        let env = self.active.clone();
        if let Some(session) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.sessions.get_mut(&id))
        {
            session.resize_grid(body_grid(
                width,
                height,
                CARD_HEADER_H,
                cell_width,
                UNZOOMED,
            ));
        }
    }
}

pub(super) fn drop_ready(
    panel: gpui_kit::Stateful<gpui_kit::Div>,
    accent: u32,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    panel.drag_over::<DraggedTab>(move |style, _, _, _| style.border_color(rgb(accent)))
}
