use std::sync::Arc;

use gpui_kit::{
    div, img, px, AnyElement, IntoElement, ObjectFit, ParentElement, RenderImage, Rgba, Styled,
    StyledImage, Window,
};
use krad_domain::BackdropFit;

const MAX_TILES_PER_AXIS: f32 = 48.0;
const FIRST_FRAME: usize = 0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Extent {
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl Extent {
    const fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) size: Extent,
}

fn tile_size(frame: Extent, image: Extent) -> Extent {
    let across = frame.width / image.width;
    let down = frame.height / image.height;
    let grow = (across.max(down) / MAX_TILES_PER_AXIS).max(1.0);
    Extent {
        width: image.width * grow,
        height: image.height * grow,
    }
}

#[must_use]
pub(crate) fn tiles(frame: Extent, image: Extent) -> Vec<Placement> {
    if frame.is_empty() || image.is_empty() {
        return Vec::new();
    }
    let size = tile_size(frame, image);
    let count = |whole: f32, part: f32| {
        let mut steps = 0_u16;
        let mut covered = 0.0;
        while covered < whole {
            steps = steps.saturating_add(1);
            covered += part;
        }
        steps
    };
    let columns = count(frame.width, size.width);
    let rows = count(frame.height, size.height);
    (0..rows)
        .flat_map(|row| {
            (0..columns).map(move |column| Placement {
                left: f32::from(column) * size.width,
                top: f32::from(row) * size.height,
                size,
            })
        })
        .collect()
}

const fn object_fit(fit: BackdropFit) -> Option<ObjectFit> {
    match fit {
        BackdropFit::Fill => Some(ObjectFit::Cover),
        BackdropFit::Fit => Some(ObjectFit::Contain),
        BackdropFit::Center => Some(ObjectFit::None),
        BackdropFit::Tile => None,
    }
}

pub(crate) struct BackdropLayer {
    pub(crate) image: Arc<RenderImage>,
    pub(crate) fit: BackdropFit,
    pub(crate) opacity: f32,
    pub(crate) veil: Rgba,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Viewport {
    size: Extent,
    scale: f32,
}

impl Viewport {
    pub(crate) fn of(window: &Window) -> Self {
        let viewport = window.viewport_size();
        Self {
            size: Extent {
                width: viewport.width.as_f32(),
                height: viewport.height.as_f32(),
            },
            scale: window.scale_factor().max(f32::EPSILON),
        }
    }
}

fn image_extent(image: &RenderImage, scale: f32) -> Extent {
    let pixels = image.size(FIRST_FRAME);
    let logical = |device: i32| f32::from(u16::try_from(device).unwrap_or(u16::MAX)) / scale;
    Extent {
        width: logical(pixels.width.0),
        height: logical(pixels.height.0),
    }
}

fn tiled(image: &Arc<RenderImage>, viewport: Viewport) -> Vec<AnyElement> {
    tiles(viewport.size, image_extent(image, viewport.scale))
        .into_iter()
        .map(|placed| {
            img(Arc::clone(image))
                .absolute()
                .left(px(placed.left))
                .top(px(placed.top))
                .w(px(placed.size.width))
                .h(px(placed.size.height))
                .object_fit(ObjectFit::Fill)
                .into_any_element()
        })
        .collect()
}

pub(crate) fn backdrop_layer(layer: &BackdropLayer, viewport: Viewport) -> AnyElement {
    let pictures = match object_fit(layer.fit) {
        Some(fit) => vec![img(Arc::clone(&layer.image))
            .absolute()
            .inset_0()
            .size_full()
            .object_fit(fit)
            .into_any_element()],
        None => tiled(&layer.image, viewport),
    };
    div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .inset_0()
                .opacity(layer.opacity)
                .children(pictures),
        )
        .child(div().absolute().inset_0().bg(layer.veil))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Extent = Extent {
        width: 1_000.0,
        height: 500.0,
    };
    const SQUARE: Extent = Extent {
        width: 100.0,
        height: 100.0,
    };

    fn same(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn every_fit_but_tile_is_left_to_the_image_element() {
        assert!(matches!(
            object_fit(BackdropFit::Fill),
            Some(ObjectFit::Cover)
        ));
        assert!(matches!(
            object_fit(BackdropFit::Fit),
            Some(ObjectFit::Contain)
        ));
        assert!(matches!(
            object_fit(BackdropFit::Center),
            Some(ObjectFit::None)
        ));
        assert!(object_fit(BackdropFit::Tile).is_none());
    }

    #[test]
    fn tiles_repeat_at_natural_size_until_the_frame_is_covered() {
        let placed = tiles(FRAME, SQUARE);
        assert_eq!(placed.len(), 10 * 5);
        assert!(placed.iter().all(|tile| tile.size == SQUARE));
        let last = placed.last().copied();
        assert_eq!(last.map(|tile| (tile.left, tile.top)), Some((900.0, 400.0)));
    }

    #[test]
    fn a_tiny_tile_grows_so_the_tile_count_stays_bounded() {
        let dot = Extent {
            width: 1.0,
            height: 1.0,
        };
        let placed = tiles(FRAME, dot);
        let per_row = placed.iter().filter(|tile| same(tile.top, 0.0)).count();
        assert!(per_row <= 49, "{per_row}");
        assert!(placed.len() <= 49 * 49);
    }

    #[test]
    fn an_empty_frame_or_image_places_nothing() {
        let empty = Extent {
            width: 0.0,
            height: 10.0,
        };
        assert_eq!(tiles(empty, SQUARE), Vec::<Placement>::new());
        assert_eq!(tiles(FRAME, empty), Vec::<Placement>::new());
    }
}
