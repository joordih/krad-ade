mod changes;
mod files;
mod search;
mod sections;
mod terminals;

pub(super) use search::SideSearch;

use gpui_kit::{
    div, px, AnyElement, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window,
};
use krad_domain::{EnvironmentView, SidePanelLayout, SIDE_PANEL_DEFAULT_W};
use std::time::Instant;

use super::drag::DragState;
use super::sidebar::PanelReveal;
use super::EnvironmentShell;
use crate::actions::ToggleSidePanel;
use crate::components::card;
use crate::theme::{palette, WINDOW_INSET};

const WIDTH_HANDLE_W: f32 = 6.0;

#[derive(Clone, Copy)]
pub(super) enum SidePanelDrag {
    Width { mouse_start: f32, width_start: f32 },
}

#[derive(Clone, Copy)]
struct PanelFrame {
    width: f32,
    files_h: f32,
}

impl EnvironmentShell {
    fn side_layout(&self) -> Option<&SidePanelLayout> {
        self.envs
            .get(&self.active)
            .map(|live| &live.data.side_panel)
    }

    fn edit_side_layout(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut SidePanelLayout),
    ) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            edit(&mut live.data.side_panel);
        }
        cx.notify();
        self.store();
    }

    pub(super) fn toggle_side_panel(&mut self, cx: &mut Context<Self>) {
        if self.active_view() == EnvironmentView::Notes {
            self.toggle_notes_sidebar(cx);
            return;
        }
        self.edit_side_layout(cx, |layout| layout.open = !layout.open);
    }

    pub(super) fn on_toggle_side_panel(
        &mut self,
        _: &ToggleSidePanel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_side_panel(cx);
    }

    pub(super) fn side_panel_open(&self) -> bool {
        self.active_view() != EnvironmentView::Notes
            && self.side_layout().is_some_and(|layout| layout.open)
    }

    pub(super) fn right_sidebar_open(&self) -> bool {
        if self.active_view() == EnvironmentView::Notes {
            return self.notes_sidebar_open();
        }
        self.side_panel_open()
    }

    pub(super) fn side_panel_width(&self) -> f32 {
        self.side_layout()
            .map_or(SIDE_PANEL_DEFAULT_W, |layout| layout.width)
    }

    pub(super) fn sync_side_panel(&mut self, now: Instant) -> bool {
        let view = self.active_view();
        let switched = view != self.side_panel_view;
        self.side_panel_view = view;
        if self.side_reveal.is_open() != self.side_panel_open() {
            if switched {
                self.side_reveal = PanelReveal::new(self.side_panel_open());
            } else {
                self.side_reveal.toggle(now);
            }
        }
        self.side_reveal.advance(now)
    }

    pub(super) const fn side_panel_moving(&self) -> bool {
        self.side_reveal.is_moving()
    }

    pub(super) fn side_panel_extent(&self) -> f32 {
        let width = self
            .side_layout()
            .map_or(0.0, |layout| layout.width + WINDOW_INSET);
        width * self.side_reveal.fraction()
    }

    fn panel_frame(&self, window: &Window) -> Option<PanelFrame> {
        let layout = self.side_layout()?;
        let (_root_w, _) = self.root_size(window);
        let row_h = self.canvas_size(window).1;
        Some(PanelFrame {
            width: layout.width,
            files_h: row_h,
        })
    }

    pub(super) fn side_panel(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let shown = self.side_reveal.fraction();
        if shown <= 0.0 {
            return None;
        }
        let frame = self.panel_frame(window)?;
        let pal = palette(cx);
        let panel = card(pal, cx).size_full().child(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .min_h_0()
                .h(px(frame.files_h))
                .child(self.side_section(cx)),
        );
        Some(
            div()
                .relative()
                .flex_none()
                .h_full()
                .w(px(self.side_panel_extent()))
                .ml(px(-WINDOW_INSET))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(WINDOW_INSET))
                        .w(px(frame.width))
                        .opacity(shown)
                        .child(panel)
                        .child(Self::width_handle(frame.width, cx)),
                )
                .into_any_element(),
        )
    }

    fn width_handle(width: f32, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .w(px(WIDTH_HANDLE_W))
            .cursor_col_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.drag = DragState::SidePanel(SidePanelDrag::Width {
                        mouse_start: event.position.x.as_f32(),
                        width_start: width,
                    });
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(super) fn drag_side_panel(&mut self, x: f32, _y: f32, cx: &mut Context<Self>) -> bool {
        let DragState::SidePanel(drag) = self.drag else {
            return false;
        };
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            let layout = &mut live.data.side_panel;
            match drag {
                SidePanelDrag::Width {
                    mouse_start,
                    width_start,
                } => layout.set_width(width_start + (mouse_start - x)),
            }
        }
        cx.notify();
        true
    }
}
