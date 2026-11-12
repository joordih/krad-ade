use gpui_fps::fps_monitor;
use gpui_kit::base::TextSelectionLayer;
use gpui_kit::prelude::FluentBuilder as _;
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
            (self.root_size(window).1 - top - WINDOW_INSET - self.usage_bar_room()).max(1.0),
        )
    }

    fn view_card(
        &mut self,
        view: EnvironmentView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _pal = palette(cx);
        if let Some(extension) = self.extension_main_view(cx) {
            return extension;
        }
        match view {
            EnvironmentView::Sessions => self.session_pane(cx),
            EnvironmentView::Notes => self.notes_view(cx),
            EnvironmentView::Automations => self.automations_view(window, cx),
            EnvironmentView::Kraddies => self.kraddies_view(window, cx),
            EnvironmentView::Review => self.review_view(window, cx),
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
                            Some(MouseButton::Left) => {
                                shell.follow_activity_drag(y, cx);
                                shell.drag_in_terminal(x, y, cx);
                            }
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
                        .update(cx, EnvironmentShell::release_captured_drag)
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
            .overflow_hidden()
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
        let viewer_extent = viewer.as_ref().map_or(0.0, |_| viewer_width + WINDOW_INSET);
        let beside_pane = viewer_extent + self.side_panel_extent();
        let view = self.view_card(view, window, cx);
        let view = self.content_shift.wrap(view, Instant::now());
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
            .child(self.top_bar(window, cx))
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
            .child(self.activity_handle(beside_pane, cx))
            .into_any_element()
    }
}

impl EnvironmentShell {
    fn sync_extensions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_extension_prompts(window, cx);
        self.sync_extension_fields(window, cx);
        self.sync_extension_events(cx);
        self.sync_app_events();
        self.sync_extension_tabs(cx);
        self.sync_extension_popover_focus(window, cx);
    }
}

impl EnvironmentShell {
    fn sync_frame_state(
        &mut self,
        view: EnvironmentView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_file_tree(cx);
        self.ensure_structured_chats(window, cx);
        self.sync_composer_rows(window, cx);
        self.sync_file_tabs(window, cx);
        self.flush_workspace_edits(window, cx);
        self.sync_extensions(window, cx);
        self.sync_changes_tabs(cx);
        self.sync_system_appearance(cx);
        self.track_cursor_jumps(cx);
        self.sync_history(cx);
        self.sync_commit_box(window, cx);
        self.sync_notes_view(view, window, cx);
        self.ensure_chat_sessions(cx);
        self.sync_session_panes();
        self.end_hidden_anchors();
        self.sync_menu_focus(window, cx);
        self.sync_model_picker_focus(window, cx);
        self.sync_focus_returns(window, cx);
        self.sync_ask_focus(window, cx);
        self.sync_terminal_images(window);
        self.probe_review(cx);
        self.sync_window_background(window, cx);
        self.sync_backdrop(window, cx);
    }
}

impl Render for EnvironmentShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = palette(cx);
        self.frame_inset = content_inset(window);
        let view = self.active_view();
        self.sync_frame_state(view, window, cx);
        let now = Instant::now();
        self.content_shift.follow(self.shown_content(view), now);
        if self.advance_motion(now, window, cx) {
            window.request_animation_frame();
        }
        self.canvas_px = self.canvas_size(window);
        let settled = !self.sidebar.is_moving()
            && !self.side_panel_moving()
            && !self.activity_reveal.is_moving()
            && !self.drag.resizes_terminals();
        if settled {
            self.sync_activity_grid(window);
            if view == EnvironmentView::Sessions && !self.editing() {
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
            .map(|root| Self::with_gestures(root, cx))
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
            .child(match self.settings_page(window, cx) {
                Some(settings) if self.gestures.settings.offset() > 0.0 => {
                    let behind = self.workspace_row(view, window, cx);
                    self.sliding_settings(settings, behind, cx)
                }
                Some(settings) => settings,
                None => self.workspace_row(view, window, cx),
            })
            .children(self.usage_bar(cx))
            .children(self.sidebar_chrome(cx))
            .children(self.extension_popover_backdrop(cx))
            .child(self.toast_stack(viewport.width.as_f32(), viewport.height.as_f32(), cx))
            .children(self.toast_placing_overlay(
                viewport.width.as_f32(),
                viewport.height.as_f32(),
                cx,
            ))
            .children(self.extension_progress(cx))
            .child(self.palette_overlay(viewport.width.as_f32(), pal, cx))
            .children(self.agent_composer_sheet(cx))
            .children(self.template_picker_sheet(cx))
            .children(self.archive_sheet(cx))
            .children(self.variants_draft_sheet(cx))
            .children(self.variants_view_sheet(cx))
            .children(self.docs_generate_sheet(cx))
            .children(self.delete_file_dialog(cx))
            .children(self.session_changes_sheet(cx))
            .children(self.discard_dialog(cx))
            .children(self.lead_ending_dialog(cx))
            .children(self.signature_overlay(window, cx))
            .children(self.editor_popover_overlay(window, cx))
            .children(self.template_popup_overlay(window, cx))
            .children(self.context_menu_overlay(window, cx))
            .children(self.model_picker_overlay(window, cx))
            .children(self.effort_popover_overlay(window, cx))
            .children(self.attention_inbox_overlay(window, cx))
            .children(self.kraddie_editor_sheet(cx))
            .children(self.kraddie_studio_sheet(cx))
            .children(self.name_dialog_sheet(window, cx))
            .children(self.trust_sheet(cx))
            .children(self.host_dialog_sheet(window, cx))
            .children(self.toolchain_entry_sheet(cx))
            .children(self.harness_onboarding_sheet(cx))
            .child(self.close_dialog(cx));
        let root = if self.fps_hud.is_visible() {
            root.child(fps_monitor(window, cx))
        } else {
            root
        };
        let image = self.backdrop_layer(krad_domain::BackdropPlace::Window, cx);
        window_frame(
            root.into_any_element(),
            window,
            pal,
            (window_backdrop(pal, cx), image),
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
