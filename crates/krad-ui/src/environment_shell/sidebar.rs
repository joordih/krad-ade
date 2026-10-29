mod bars;
mod environment;
mod views;

use gpui_kit::{
    div, px, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use std::time::Instant;

use super::EnvironmentShell;
use crate::actions::ToggleSidebar;
use crate::components::{icon, tooltip_view, Button, IconName};
use crate::motion::Spring;
use crate::theme::{palette, ICON_BUTTON_SM, ICON_XS, RADIUS_XS, SPACE_2, WINDOW_INSET};
use crate::title_bar::{LEADING_CONTROLS_END_PX, TITLE_BAR_HEIGHT_PX};

const SIDEBAR_LIST_ID: &str = "sidebar-list";
const PEEK_EDGE_W: f32 = 8.0;
const PEEK_ARROW_W: f32 = 10.0;
const PEEK_ARROW_ID: &str = "sidebar-peek-arrow";
const PEEK_PIN_TOOLTIP: &str = "Keep the sidebar open";
const PEEK_ARROW_H: f32 = 36.0;
const PEEK_LEAVE_SLACK: f32 = 24.0;
const BTN_TOGGLE_SIDEBAR: &str = "toggle-sidebar";
const TOOLTIP_TOGGLE_SIDEBAR: &str = "Show or hide the sidebar";
const TOGGLE_GAP: f32 = SPACE_2;
const SIDEBAR_TOGGLE_LEFT: f32 = LEADING_CONTROLS_END_PX + TOGGLE_GAP;
pub(super) const SIDEBAR_TOGGLE_END: f32 = SIDEBAR_TOGGLE_LEFT + ICON_BUTTON_SM + TOGGLE_GAP;

pub(super) const SIDEBAR_WIDTH: f32 = 240.0;
const FOOTER_H: f32 = 36.0;
const SIDEBAR_RESPONSE_S: f32 = 0.35;
const MAX_FRAME_S: f32 = 1.0 / 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Peek {
    Off,
    Peeking,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarState {
    Open,
    Closed,
}

impl SidebarState {
    const fn fraction(self) -> f32 {
        match self {
            Self::Open => 1.0,
            Self::Closed => 0.0,
        }
    }

    const fn toggled(self) -> Self {
        match self {
            Self::Open => Self::Closed,
            Self::Closed => Self::Open,
        }
    }
}

pub(super) struct PanelReveal {
    state: SidebarState,
    spring: Spring,
    last_frame: Option<Instant>,
}

impl PanelReveal {
    pub(super) fn open() -> Self {
        Self::new(true)
    }

    pub(super) fn new(open: bool) -> Self {
        let state = if open {
            SidebarState::Open
        } else {
            SidebarState::Closed
        };
        Self {
            state,
            spring: Spring::at_rest(state.fraction(), SIDEBAR_RESPONSE_S),
            last_frame: None,
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.state == SidebarState::Open
    }

    pub(super) const fn fraction(&self) -> f32 {
        self.spring.value()
    }

    pub(super) const fn is_moving(&self) -> bool {
        self.last_frame.is_some()
    }

    pub(super) fn toggle(&mut self, now: Instant) {
        self.state = self.state.toggled();
        self.spring.set_target(self.state.fraction());
        self.last_frame = Some(now);
    }

    pub(super) fn advance(&mut self, now: Instant) -> bool {
        let Some(last) = self.last_frame else {
            return false;
        };
        let elapsed = now.duration_since(last).as_secs_f32().min(MAX_FRAME_S);
        self.spring.step(elapsed);
        self.last_frame = (!self.spring.is_settled()).then_some(now);
        self.last_frame.is_some()
    }
}

impl EnvironmentShell {
    pub(super) fn sidebar_toggle(cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .top(px(WINDOW_INSET))
            .left(px(SIDEBAR_TOGGLE_LEFT))
            .h(px(TITLE_BAR_HEIGHT_PX))
            .flex()
            .items_center()
            .child(
                div().occlude().child(
                    Button::new(BTN_TOGGLE_SIDEBAR)
                        .icon(IconName::SidebarLeft)
                        .small()
                        .tooltip(TOOLTIP_TOGGLE_SIDEBAR)
                        .shortcut(Box::new(ToggleSidebar))
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| this.toggle_sidebar(cx))),
                ),
            )
            .into_any_element()
    }

    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar.toggle(Instant::now());
        self.sidebar_peek = Peek::Off;
        cx.notify();
    }

    pub(super) fn follow_pointer_for_peek(&mut self, x: f32, cx: &mut Context<Self>) {
        if self.settings_ui.is_some() {
            return;
        }
        match self.sidebar_peek {
            Peek::Off => {
                if !self.sidebar.is_open() && !self.sidebar.is_moving() && x <= PEEK_EDGE_W {
                    self.sidebar.toggle(Instant::now());
                    self.sidebar_peek = Peek::Peeking;
                    cx.notify();
                }
            }
            Peek::Peeking => {
                if x > WINDOW_INSET + SIDEBAR_WIDTH + PEEK_LEAVE_SLACK {
                    if self.sidebar.is_open() {
                        self.sidebar.toggle(Instant::now());
                    }
                    self.sidebar_peek = Peek::Off;
                    cx.notify();
                }
            }
        }
    }

    fn sidebar_peek(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let peeking = self.sidebar_peek == Peek::Peeking;
        if self.sidebar.is_open() && !peeking {
            return None;
        }
        let pal = palette(cx);
        Some(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .flex()
                .items_center()
                .child(
                    div()
                        .id(PEEK_ARROW_ID)
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(PEEK_ARROW_W))
                        .h(px(PEEK_ARROW_H))
                        .rounded_r(px(RADIUS_XS))
                        .cursor_pointer()
                        .occlude()
                        .bg(pal.bg_hover)
                        .hover(|style| style.bg(pal.bg_selected))
                        .tooltip(|_, cx| tooltip_view(PEEK_PIN_TOOLTIP.into(), cx))
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| this.pin_sidebar(cx)))
                        .child(icon(IconName::ChevronRight, ICON_XS, pal.text_muted)),
                )
                .into_any_element(),
        )
    }

    fn pin_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_peek == Peek::Off && !self.sidebar.is_open() {
            self.sidebar.toggle(Instant::now());
        }
        self.sidebar_peek = Peek::Off;
        cx.notify();
    }

    pub(super) fn sidebar_chrome(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.settings_ui.is_some() {
            return Vec::new();
        }
        let mut chrome: Vec<AnyElement> = self.sidebar_peek(cx).into_iter().collect();
        if self.sidebar_toggle_shown() {
            chrome.push(Self::sidebar_toggle(cx));
        }
        chrome
    }

    fn sidebar_toggle_shown(&self) -> bool {
        self.sidebar.is_open() && self.sidebar_peek == Peek::Off
    }

    pub(super) fn on_toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_sidebar(cx);
    }

    pub(super) fn sidebar_width(&self) -> f32 {
        (SIDEBAR_WIDTH + WINDOW_INSET) * self.sidebar.fraction()
    }

    fn terminal_list(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id(SIDEBAR_LIST_ID)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(self.terminal_groups(cx))
            .into_any_element()
    }

    fn sidebar_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let views = Some(self.view_list(cx));
        let list = self.terminal_list(cx);
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(Self::sidebar_header())
            .child(self.environment_header(cx))
            .children(views)
            .child(list)
            .child(self.sidebar_footer(cx))
    }

    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let shown = self.sidebar.fraction();
        if shown <= 0.0 {
            return div().into_any_element();
        }
        let width = self.sidebar_width();
        div()
            .relative()
            .flex_none()
            .h_full()
            .w(px(width))
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(width - SIDEBAR_WIDTH - WINDOW_INSET))
                    .w(px(SIDEBAR_WIDTH))
                    .opacity(shown)
                    .child(self.sidebar_content(cx)),
            )
            .into_any_element()
    }
}
