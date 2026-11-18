use gpui_kit::{Context, Window};
use krad_runtime::TerminalAction;
use krad_session::{CellPoint, MouseAction};

mod edge;

pub(in crate::environment_shell) use edge::{edge_handle, EdgeDrag, FixedSide};

use super::terminal_body::BodyGeometry;
use super::EnvironmentShell;
use crate::actions::{NewChat, ToggleFps, TogglePalette};

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
    Resize(EdgeDrag),
}

impl DragState {
    pub(in crate::environment_shell) const fn resizes_terminals(&self) -> bool {
        matches!(self, Self::Resize(_))
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

    pub(super) fn on_new_chat(&mut self, _: &NewChat, _: &mut Window, cx: &mut Context<Self>) {
        self.add_new_tab(cx);
    }

    pub(super) fn canvas_size(&self, window: &Window) -> (f32, f32) {
        let (width, height) = self.content_size(window);
        (width, (height - self.activity_room()).max(1.0))
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
                let at = geometry.cell_at(x, y);
                self.act_on_terminal(&env, id, TerminalAction::ExtendSelection(at));
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
            DragState::Resize(edge) => edge.reshapes_layout(),
            DragState::Select { .. } | DragState::SelectCode | DragState::Report { .. } => false,
        };
        match self.drag {
            DragState::Select { id, .. } => self.copy_finished_selection(id, cx),
            DragState::Report { id, cell, .. } => self.report_click(id, MouseAction::Release, cell),
            _ => {}
        }
        self.drag = DragState::None;
        cx.notify();
        if reshaped_layout {
            self.save_view();
        }
    }
}
