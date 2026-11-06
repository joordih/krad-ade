use gpui_kit::{Context, Window};
use krad_session::{CellPoint, MouseAction};

use super::activity::ActivityDrag;
use super::layout::PanelDrag;
use super::side_panel::SidePanelDrag;
use super::sidebar::SidebarDrag;
use super::terminal_body::BodyGeometry;
use super::EnvironmentShell;
use crate::actions::{NewTerminal, ToggleFps, TogglePalette};

#[derive(Clone)]
pub(in crate::environment_shell) enum DragState {
    None,
    Select {
        id: krad_domain::TerminalId,
        geometry: BodyGeometry,
    },
    Report {
        id: krad_domain::TerminalId,
        geometry: BodyGeometry,
        cell: CellPoint,
    },
    SelectCode,
    ResizePanel(PanelDrag),
    ResizeActivity(ActivityDrag),
    ResizeSidebar(SidebarDrag),
    SidePanel(SidePanelDrag),
    ResizePane {
        divider: krad_domain::DividerId,
        axis: krad_domain::Axis,
        frame: krad_domain::UnitRect,
    },
}

impl DragState {
    pub(in crate::environment_shell) const fn resizes_terminals(&self) -> bool {
        matches!(
            self,
            Self::ResizePanel(_)
                | Self::ResizeActivity(_)
                | Self::ResizeSidebar(_)
                | Self::SidePanel(_)
                | Self::ResizePane { .. }
        )
    }
}

impl EnvironmentShell {
    pub(super) fn on_toggle_fps(&mut self, _: &ToggleFps, _: &mut Window, cx: &mut Context<Self>) {
        self.fps_hud = self.fps_hud.toggle();
        cx.notify();
    }

    pub(super) fn on_toggle_palette(
        &mut self,
        _: &TogglePalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_palette(window, cx);
    }

    pub(super) fn on_new_terminal(
        &mut self,
        _: &NewTerminal,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_new_tab(cx);
    }

    pub(super) fn canvas_size(&self, window: &Window) -> (f32, f32) {
        let (width, height) = self.content_size(window);
        (width, (height - self.activity_room()).max(1.0))
    }

    fn drag_panel_edges(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        self.drag_side_panel(x, y, cx) || self.drag_pane_divider(x, y, cx)
    }

    pub(in crate::environment_shell) fn drag_to(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if matches!(self.drag, DragState::None) || self.drag_panel_edges(x, y, cx) {
            return;
        }
        match &self.drag {
            DragState::None
            | DragState::SelectCode
            | DragState::SidePanel(_)
            | DragState::ResizeActivity(_)
            | DragState::Select { .. }
            | DragState::Report { .. }
            | DragState::ResizePane { .. } => return,
            DragState::ResizePanel(drag) => self.panel_widths.set(drag.panel, drag.width_at(x)),
            DragState::ResizeSidebar(drag) => self.sidebar_w = drag.width_at(x),
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn drag_in_terminal(
        &mut self,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        match self.drag {
            DragState::Select { id, geometry } => {
                let env = self.active.clone();
                if let Some(session) = self
                    .envs
                    .get_mut(&env)
                    .and_then(|live| live.sessions.get_mut(&id))
                {
                    session.engine.extend_selection(geometry.cell_at(x, y));
                }
                cx.notify();
            }
            DragState::Report { id, geometry, cell } => {
                let moved = geometry.cell_at(x, y);
                if moved != cell {
                    self.report_click(id, MouseAction::Drag, moved);
                    self.drag = DragState::Report {
                        id,
                        geometry,
                        cell: moved,
                    };
                }
            }
            _ => {}
        }
    }

    pub(in crate::environment_shell) fn end_drag(&mut self, cx: &mut Context<Self>) {
        let reshaped_layout = match self.drag {
            DragState::None => return,
            DragState::SidePanel(_) | DragState::ResizeSidebar(_) => true,
            DragState::Select { .. }
            | DragState::SelectCode
            | DragState::Report { .. }
            | DragState::ResizePanel(_)
            | DragState::ResizeActivity(_)
            | DragState::ResizePane { .. } => false,
        };
        match self.drag {
            DragState::Select { id, .. } => self.copy_finished_selection(id, cx),
            DragState::Report { id, cell, .. } => self.report_click(id, MouseAction::Release, cell),
            _ => {}
        }
        self.drag = DragState::None;
        cx.notify();
        if reshaped_layout {
            self.store();
        }
    }
}
