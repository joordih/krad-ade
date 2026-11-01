use gpui_fps::fps_monitor;
use gpui_kit::base::TextSelectionLayer;
use gpui_kit::{
    canvas, div, px, rgb, AnyElement, Context, DispatchPhase, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Render, Styled,
    Window,
};
use krad_domain::EnvironmentView;
use std::time::Instant;

use super::code_viewer::VIEWER_MIN_W;
use super::drag::DragState;
use super::EnvironmentShell;
use crate::actions::{MinimizeWindow, ZoomWindow};
use crate::theme::{palette, window_backdrop, TOOLBAR_H, WINDOW_INSET};
use crate::window_frame::{content_inset, window_frame};

const PANE_MIN_W: f32 = 320.0;
const WIDTH_HANDLE_W: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SidePanel {
    Viewer,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(super) struct PanelWidths {
    viewer: Option<f32>,
}

impl PanelWidths {
    pub(super) fn set(&mut self, panel: SidePanel, width: f32) {
        match panel {
            SidePanel::Viewer => self.viewer = Some(width.max(VIEWER_MIN_W)),
        }
    }

    pub(super) fn viewer_in(self, row: f32) -> Option<f32> {
        let max = (row - PANE_MIN_W - WINDOW_INSET).max(VIEWER_MIN_W).min(row);
        self.viewer
            .map(|width| width.clamp(VIEWER_MIN_W.min(max), max))
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct PanelDrag {
    pub(super) panel: SidePanel,
    mouse_start: f32,
    width_start: f32,
}

impl PanelDrag {
    pub(super) fn width_at(self, x: f32) -> f32 {
        self.width_start + (self.mouse_start - x)
    }
}

pub(super) fn width_handle(
    panel: SidePanel,
    width: f32,
    cx: &mut Context<EnvironmentShell>,
) -> AnyElement {
    div()
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(WIDTH_HANDLE_W))
        .cursor_col_resize()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(
                move |this: &mut EnvironmentShell, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.drag = DragState::ResizePanel(PanelDrag {
                        panel,
                        mouse_start: event.position.x.as_f32(),
                        width_start: width,
                    });
                    cx.notify();
                },
            ),
        )
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn content_origin(&self) -> (f32, f32) {
        (
            WINDOW_INSET + self.sidebar_width(),
            WINDOW_INSET + TOOLBAR_H + WINDOW_INSET,
        )
    }

    pub(super) fn window_origin(&self) -> (f32, f32) {
        let (left, top) = self.content_origin();
        (left + self.frame_inset.0, top + self.frame_inset.1)
    }

    pub(super) fn root_size(&self, window: &Window) -> (f32, f32) {
        let viewport = window.viewport_size();
        (
            viewport.width.as_f32() - 2.0 * self.frame_inset.0,
            viewport.height.as_f32() - 2.0 * self.frame_inset.1,
        )
    }

    pub(super) fn content_row_width(&self, window: &Window) -> f32 {
        let (left, _) = self.content_origin();
        (self.root_size(window).0 - left - WINDOW_INSET - self.side_panel_extent()).max(1.0)
    }

    pub(super) fn content_size(&self, window: &Window) -> (f32, f32) {
        let (_, top) = self.content_origin();
        let row = self.content_row_width(window);
        let viewer = self.viewer_width(row);
        let viewer_extent = if viewer > 0.0 {
            viewer + WINDOW_INSET
        } else {
            0.0
        };
        (
            (row - viewer_extent).max(1.0),
            (self.root_size(window).1 - top - WINDOW_INSET).max(1.0),
        )
    }

    fn view_card(
        &mut self,
        view: EnvironmentView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _pal = palette(cx);
        match view {
            EnvironmentView::Sessions => self.session_pane(cx),
            EnvironmentView::Notes => self.notes_view(cx),
            EnvironmentView::Review => self.review_view(cx),
        }
    }

    fn on_root_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button == Some(MouseButton::Left) {
            self.drag_to(event.position.x.as_f32(), event.position.y.as_f32(), cx);
            return;
        }
        self.end_drag(cx);
    }

    fn pointer_watch(cx: &mut Context<Self>) -> AnyElement {
        let shell = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, (), window, _| {
                let moved = shell.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let (x, y) = (event.position.x.as_f32(), event.position.y.as_f32());
                    let window_h = window.viewport_size().height.as_f32();
                    moved
                        .update(cx, |shell, cx| match event.pressed_button {
                            Some(MouseButton::Left) => shell.follow_activity_drag(y, cx),
                            Some(_) => {}
                            None => {
                                shell.follow_pointer_for_peek(x, cx);
                                shell.follow_pointer_for_activity(y, window_h, cx);
                            }
                        })
                        .unwrap_or_default();
                });
                let released = shell.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    released
                        .update(cx, EnvironmentShell::release_activity_drag)
                        .unwrap_or_default();
                });
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    fn workspace_row(
        &mut self,
        view: EnvironmentView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .p(px(WINDOW_INSET))
            .child(self.sidebar(cx))
            .child(self.content_column(view, window, cx))
            .into_any_element()
    }

    fn content_column(
        &mut self,
        view: EnvironmentView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewer_width = self.viewer_width(self.content_row_width(window));
        let viewer = self.code_viewer(viewer_width, cx);
        let view = self.view_card(view, window, cx);
        let pane = self.main_pane(view, cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap(px(WINDOW_INSET))
            .pb(px(self.activity_room()))
            .child(self.top_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .gap(px(WINDOW_INSET))
                    .child(pane)
                    .children(viewer)
                    .children(self.side_panel(window, cx)),
            )
            .children(self.activity_panel(cx))
            .child(self.activity_handle(cx))
            .into_any_element()
    }
}

impl Render for EnvironmentShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = palette(cx);
        self.frame_inset = content_inset(window);
        let view = self.active_view();
        self.sync_file_tree(cx);
        self.ensure_structured_chats(window, cx);
        self.sync_file_tabs(window, cx);
        self.sync_changes_tabs(cx);
        self.sync_notes_view(view, window, cx);
        self.ensure_chat_sessions(cx);
        self.sync_session_panes();
        self.sync_menu_focus(window, cx);
        self.sync_model_picker_focus(window, cx);
        self.sync_ask_focus(window, cx);
        self.sync_terminal_images(window);
        self.probe_review(cx);
        self.sync_window_background(window, cx);
        let now = Instant::now();
        if self.sidebar.advance(now)
            | self.sync_side_panel(now)
            | self.sync_activity(now)
            | self.pane_drag.advance(now)
            | self.sync_chat_landings(now)
            | self.glide_chats(now)
            | self.tick_arcade(now, cx)
        {
            window.request_animation_frame();
        }
        self.canvas_px = self.canvas_size(window);
        let settled = !self.sidebar.is_moving()
            && !self.side_panel_moving()
            && !self.activity_reveal.is_moving()
            && !self.drag.resizes_terminals();
        if settled {
            self.sync_activity_grid(window);
            if view == EnvironmentView::Sessions {
                self.sync_session_grid(window);
            }
        }
        let viewport = window.viewport_size();
        let root = div()
            .on_action(cx.listener(Self::on_close_window))
            .on_action(cx.listener(Self::on_quit))
            .on_action(|_: &MinimizeWindow, window, _| window.minimize_window())
            .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
            .on_mouse_move(cx.listener(Self::on_root_mouse_move))
            .child(Self::pointer_watch(cx))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.end_drag(cx);
                    this.end_tab_drag(cx);
                }),
            )
            .flex()
            .flex_col()
            .size_full()
            .relative()
            .text_color(rgb(pal.text_default))
            .child(TextSelectionLayer)
            .child(match self.settings_page(cx) {
                Some(settings) => settings,
                None => self.workspace_row(view, window, cx),
            })
            .children(self.sidebar_chrome(cx))
            .child(self.toast_stack(cx))
            .child(self.palette_overlay(viewport.width.as_f32(), pal, cx))
            .children(self.agent_composer_sheet(cx))
            .children(self.docs_generate_sheet(cx))
            .children(self.delete_file_dialog(cx))
            .children(self.context_menu_overlay(window, cx))
            .children(self.model_picker_overlay(window, cx))
            .children(self.name_dialog_sheet(cx))
            .child(self.close_dialog(cx));
        let root = if self.fps_hud.is_visible() {
            root.child(fps_monitor(window, cx))
        } else {
            root
        };
        window_frame(
            root.into_any_element(),
            window,
            pal,
            window_backdrop(pal, cx),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: f32, b: f32) -> bool {
        (a - b).abs() < f32::EPSILON
    }

    #[test]
    fn the_viewer_follows_the_card_until_dragged_and_then_leaves_room_for_the_view() {
        let mut widths = PanelWidths::default();
        assert_eq!(widths.viewer_in(1_200.0), None);
        widths.set(SidePanel::Viewer, 5_000.0);
        assert_eq!(
            widths.viewer_in(1_200.0),
            Some(1_200.0 - PANE_MIN_W - WINDOW_INSET)
        );
        widths.set(SidePanel::Viewer, 0.0);
        assert_eq!(widths.viewer_in(1_200.0), Some(VIEWER_MIN_W));
    }

    #[test]
    fn dragging_a_left_edge_left_widens_its_card() {
        let drag = PanelDrag {
            panel: SidePanel::Viewer,
            mouse_start: 800.0,
            width_start: 400.0,
        };
        assert!(same(drag.width_at(700.0), 500.0));
        assert!(same(drag.width_at(900.0), 300.0));
    }
}
