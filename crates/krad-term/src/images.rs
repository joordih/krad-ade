use libghostty_vt::kitty::graphics::ImageFormat;
use libghostty_vt::render::{ImageIterator, ImageLayer, ImageProtocol, Snapshot};
use libghostty_vt::{Error, Terminal};

const RGBA_CHANNELS: usize = 4;
const RGB_CHANNELS: usize = 3;
const GRAY_ALPHA_CHANNELS: usize = 2;
const OPAQUE: u8 = u8::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageDepth {
    BelowText,
    AboveText,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SourceRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TermImage {
    pub id: u32,
    pub generation: u64,
    pub depth: ImageDepth,
    pub z: i32,
    pub col: i32,
    pub row: i32,
    pub offset_x: u32,
    pub offset_y: u32,
    pub width: u32,
    pub height: u32,
    pub source: SourceRect,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ImagePixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

pub(crate) struct ImageReader {
    iterator: ImageIterator<'static>,
    generation: Option<u64>,
    placements: Vec<TermImage>,
}

impl ImageReader {
    pub(crate) fn new() -> Result<Self, Error> {
        Ok(Self {
            iterator: ImageIterator::new()?,
            generation: None,
            placements: Vec::new(),
        })
    }

    pub(crate) fn placements(
        &mut self,
        snapshot: &Snapshot<'static, '_>,
    ) -> Result<Vec<TermImage>, Error> {
        let generation = snapshot.image_placements_generation()?;
        if self.generation == Some(generation) {
            return Ok(self.placements.clone());
        }
        let mut placements = Vec::new();
        let mut iteration = self.iterator.update(snapshot)?;
        while let Some(placement) = iteration.next() {
            let Ok(ImageProtocol::Kitty) = placement.protocol() else {
                continue;
            };
            let depth = match placement.layer()? {
                ImageLayer::AboveText => ImageDepth::AboveText,
                _ => ImageDepth::BelowText,
            };
            placements.push(TermImage {
                id: placement.image_id()?,
                generation: placement.image_generation()?,
                depth,
                z: placement.z()?,
                col: placement.viewport_col()?,
                row: placement.viewport_row()?,
                offset_x: placement.cell_offset_x()?,
                offset_y: placement.cell_offset_y()?,
                width: placement.pixel_width()?,
                height: placement.pixel_height()?,
                source: SourceRect {
                    x: placement.source_x()?,
                    y: placement.source_y()?,
                    width: placement.source_width()?,
                    height: placement.source_height()?,
                },
            });
        }
        placements.sort_by_key(|image| (image.depth == ImageDepth::AboveText, image.z));
        self.generation = Some(generation);
        self.placements.clone_from(&placements);
        Ok(placements)
    }
}

fn to_bgra(format: ImageFormat, pixels: &[u8]) -> Option<Vec<u8>> {
    let channels = match format {
        ImageFormat::Rgba => RGBA_CHANNELS,
        ImageFormat::Rgb => RGB_CHANNELS,
        ImageFormat::GrayAlpha => GRAY_ALPHA_CHANNELS,
        ImageFormat::Gray => 1,
        _ => return None,
    };
    let mut bgra = Vec::with_capacity(pixels.len() / channels * RGBA_CHANNELS);
    for pixel in pixels.chunks_exact(channels) {
        let (red, green, blue, alpha) = match *pixel {
            [red, green, blue, alpha] => (red, green, blue, alpha),
            [red, green, blue] => (red, green, blue, OPAQUE),
            [gray, alpha] => (gray, gray, gray, alpha),
            [gray] => (gray, gray, gray, OPAQUE),
            _ => return None,
        };
        bgra.extend([blue, green, red, alpha]);
    }
    Some(bgra)
}

pub(crate) fn pixels(term: &Terminal<'_, '_>, id: u32) -> Option<ImagePixels> {
    let graphics = term.kitty_graphics().ok()?;
    let image = graphics.image(id)?;
    let width = image.width().ok()?;
    let height = image.height().ok()?;
    let data = image.data().ok()??;
    let bgra = to_bgra(image.format().ok()?, data)?;
    let expected = usize::try_from(width).ok()? * usize::try_from(height).ok()? * RGBA_CHANNELS;
    (bgra.len() == expected).then_some(ImagePixels {
        width,
        height,
        bgra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pixel_format_becomes_bgra() {
        assert_eq!(
            to_bgra(ImageFormat::Rgba, &[1, 2, 3, 4]),
            Some(vec![3, 2, 1, 4])
        );
        assert_eq!(
            to_bgra(ImageFormat::Rgb, &[1, 2, 3]),
            Some(vec![3, 2, 1, OPAQUE])
        );
        assert_eq!(
            to_bgra(ImageFormat::Gray, &[9]),
            Some(vec![9, 9, 9, OPAQUE])
        );
        assert_eq!(
            to_bgra(ImageFormat::GrayAlpha, &[9, 5]),
            Some(vec![9, 9, 9, 5])
        );
    }
}
