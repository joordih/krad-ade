use gpui_kit::{div, img, px, AnyElement, IntoElement, ParentElement, RenderImage, Styled, Window};
use image::{Frame, RgbaImage};
use krad_domain::TerminalId;
use krad_session::{CellPixels, ImageDepth, ImagePixels, TermImage};
use std::collections::HashMap;
use std::sync::Arc;

use super::EnvironmentShell;

#[derive(Default)]
pub(super) struct TerminalImages(HashMap<u32, (u64, Arc<RenderImage>)>);

#[derive(Clone, Copy)]
pub(super) struct ImageGrid {
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) cell_w: f32,
    pub(super) line_h: f32,
    pub(super) placed_with: CellPixels,
}

fn render_image(pixels: ImagePixels) -> Option<Arc<RenderImage>> {
    let buffer = RgbaImage::from_raw(pixels.width, pixels.height, pixels.bgra)?;
    Some(Arc::new(RenderImage::new(vec![Frame::new(buffer)])))
}

fn pixels_f32(value: u32) -> f32 {
    u16::try_from(value).map_or(f32::from(u16::MAX), f32::from)
}

fn cells_f32(value: i32) -> f32 {
    i16::try_from(value).map_or_else(
        |_| {
            if value < 0 {
                f32::from(i16::MIN)
            } else {
                f32::from(i16::MAX)
            }
        },
        f32::from,
    )
}

fn placed_element(placed: &TermImage, image: &Arc<RenderImage>, grid: ImageGrid) -> AnyElement {
    let scale_x = grid.cell_w / pixels_f32(grid.placed_with.width.max(1));
    let scale_y = grid.line_h / pixels_f32(grid.placed_with.height.max(1));
    let width = pixels_f32(placed.width) * scale_x;
    let height = pixels_f32(placed.height) * scale_y;
    let source = placed.source;
    let per_source_x = width / pixels_f32(source.width.max(1));
    let per_source_y = height / pixels_f32(source.height.max(1));
    let size = image.size(0);
    let image_w = f32::from(u16::try_from(size.width.0).unwrap_or(u16::MAX)) * per_source_x;
    let image_h = f32::from(u16::try_from(size.height.0).unwrap_or(u16::MAX)) * per_source_y;
    div()
        .absolute()
        .left(px(grid.left
            + cells_f32(placed.col) * grid.cell_w
            + pixels_f32(placed.offset_x) * scale_x))
        .top(px(grid.top
            + cells_f32(placed.row) * grid.line_h
            + pixels_f32(placed.offset_y) * scale_y))
        .w(px(width))
        .h(px(height))
        .overflow_hidden()
        .child(
            img(Arc::clone(image))
                .absolute()
                .left(px(-pixels_f32(source.x) * per_source_x))
                .top(px(-pixels_f32(source.y) * per_source_y))
                .w(px(image_w))
                .h(px(image_h)),
        )
        .into_any_element()
}

pub(super) fn image_layer(
    placements: &[TermImage],
    uploaded: Option<&TerminalImages>,
    depth: ImageDepth,
    grid: ImageGrid,
) -> Vec<AnyElement> {
    let Some(uploaded) = uploaded else {
        return Vec::new();
    };
    placements
        .iter()
        .filter(|placed| placed.depth == depth)
        .filter_map(|placed| {
            let (generation, image) = uploaded.0.get(&placed.id)?;
            (*generation == placed.generation).then(|| placed_element(placed, image, grid))
        })
        .collect()
}

impl EnvironmentShell {
    pub(super) fn sync_terminal_images(&mut self, window: &mut Window) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let mut freed: Vec<Arc<RenderImage>> = Vec::new();
        live.images.retain(|id, uploaded| {
            let kept = live.sessions.contains_key(id);
            if !kept {
                freed.extend(uploaded.0.drain().map(|(_, (_, image))| image));
            }
            kept
        });
        for (id, session) in &live.sessions {
            let placements = session.engine.images();
            if placements.is_empty() && !live.images.contains_key(id) {
                continue;
            }
            let uploaded = live.images.entry(*id).or_default();
            uploaded.0.retain(|image_id, (generation, image)| {
                let shown = placements
                    .iter()
                    .any(|placed| placed.id == *image_id && placed.generation == *generation);
                if !shown {
                    freed.push(Arc::clone(image));
                }
                shown
            });
            for placed in &placements {
                if uploaded.0.contains_key(&placed.id) {
                    continue;
                }
                let image = session
                    .engine
                    .image_pixels(placed.id)
                    .and_then(render_image);
                if let Some(image) = image {
                    uploaded.0.insert(placed.id, (placed.generation, image));
                }
            }
        }
        for image in freed {
            window.drop_image(image).unwrap_or_default();
        }
    }

    pub(super) fn terminal_images(&self, id: TerminalId) -> Option<&TerminalImages> {
        self.envs.get(&self.active)?.images.get(&id)
    }
}
