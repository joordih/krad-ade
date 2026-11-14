use krad_session::{CellPoint, GridSize};

pub(in crate::environment_shell) const TERMINAL_FONT_PX: f32 = crate::theme::TEXT_XS;
pub(in crate::environment_shell) const TERMINAL_LINE_PX: f32 = 18.0;
pub(in crate::environment_shell) const BODY_PAD_X: f32 = 12.0;
pub(in crate::environment_shell) const BODY_PAD_TOP: f32 = crate::theme::SPACE_1;
pub(in crate::environment_shell) const BODY_PAD_BOTTOM: f32 = crate::theme::SPACE_3;

#[derive(Clone, Copy)]
pub(in crate::environment_shell) struct BodyGeometry {
    pub(in crate::environment_shell) origin: (f32, f32),
    pub(in crate::environment_shell) cell_w: f32,
    pub(in crate::environment_shell) line_h: f32,
}

impl BodyGeometry {
    pub(in crate::environment_shell) fn cell_at(&self, x: f32, y: f32) -> CellPoint {
        CellPoint {
            row: cell_index(y - self.origin.1, self.line_h),
            col: cell_index(x - self.origin.0, self.cell_w),
        }
    }
}

fn cell_index(offset: f32, size: f32) -> usize {
    let index = (offset / size.max(f32::EPSILON)).floor().max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cells = index as usize;
    cells
}

pub(in crate::environment_shell) fn body_grid(
    width: f32,
    height: f32,
    header: f32,
    cell_width: f32,
    text_scale: f32,
) -> GridSize {
    let body_w = width - 2.0 * BODY_PAD_X;
    let body_h = height - header - BODY_PAD_TOP - BODY_PAD_BOTTOM;
    let (cell_w, cell_h) = (cell_width * text_scale, TERMINAL_LINE_PX * text_scale);
    GridSize {
        cols: cells_in(body_w, cell_w),
        rows: cells_in(body_h, cell_h),
        cell_width_px: whole_px(cell_w),
        cell_height_px: whole_px(cell_h),
    }
}

fn whole_px(length: f32) -> u16 {
    let rounded = length.round().clamp(0.0, f32::from(u16::MAX));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = rounded as u16;
    pixels
}

fn cells_in(length: f32, cell: f32) -> u16 {
    let count = (length / cell).floor().clamp(1.0, f32::from(u16::MAX));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cells = count as u16;
    cells
}
