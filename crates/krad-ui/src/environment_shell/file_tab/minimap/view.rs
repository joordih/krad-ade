pub(super) const LINE_PX: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MinimapView {
    pub(super) offset: f32,
    pub(super) slider_top: f32,
    pub(super) slider_height: f32,
    lines_per_slider_px: f32,
}

#[allow(clippy::cast_precision_loss)]
pub(super) fn minimap_view(lines: usize, top_line: f32, visible: f32, height: f32) -> MinimapView {
    let total = lines as f32 * LINE_PX;
    let overflow = (total - height).max(0.0);
    let last_top = (lines as f32 - visible).max(1.0);
    let offset = overflow * (top_line / last_top).clamp(0.0, 1.0);
    let step = LINE_PX - overflow / last_top;
    MinimapView {
        offset,
        slider_top: top_line * LINE_PX - offset,
        slider_height: (visible * LINE_PX).max(LINE_PX),
        lines_per_slider_px: if step > f32::EPSILON { 1.0 / step } else { 0.0 },
    }
}

impl MinimapView {
    pub(super) fn top_line_for_slider(&self, slider_top: f32) -> f32 {
        (slider_top * self.lines_per_slider_px).max(0.0)
    }

    pub(super) fn line_at(&self, y: f32) -> f32 {
        (y + self.offset) / LINE_PX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_file_never_scrolls_its_minimap() {
        let view = minimap_view(50, 10.0, 20.0, 600.0);
        assert!(view.offset.abs() < f32::EPSILON);
        assert!((view.slider_top - 30.0).abs() < f32::EPSILON);
        assert!((view.slider_height - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_long_file_scrolls_its_minimap_with_the_editor_and_drags_back_to_the_same_line() {
        let (lines, visible, height) = (1_000, 40.0, 600.0);
        let top = minimap_view(lines, 0.0, visible, height);
        let bottom = minimap_view(lines, 960.0, visible, height);
        assert!(top.offset.abs() < f32::EPSILON);
        assert!((bottom.offset - 2_400.0).abs() < 0.01);
        assert!((bottom.slider_top + bottom.slider_height - height).abs() < 0.01);
        let middle = minimap_view(lines, 480.0, visible, height);
        assert!((middle.top_line_for_slider(middle.slider_top) - 480.0).abs() < 0.01);
    }
}
