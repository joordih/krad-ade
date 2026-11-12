use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{
    AnyElement, AppContext as _, Context, PathPromptOptions, RenderImage, SharedString, Window,
    WindowBackgroundAppearance,
};
use image::imageops::fast_blur;
use image::{DynamicImage, Frame, ImageError, RgbaImage};
use krad_domain::{BackdropImage, BackdropPercent, BackdropPlace, ToastKind};
use krad_store::{forget_backdrop_image, import_backdrop_image};

use super::EnvironmentShell;
use crate::theme::{backdrop_veil, palette};
use crate::window_frame::{backdrop_layer, transparency_reduced, BackdropLayer, Viewport};

const MAX_EDGE: u32 = 2560;
const MAX_BLUR_SIGMA: f32 = 24.0;
const PICK_PROMPT: &str = "Choose a backdrop image";
const NOT_SHOWN_TEXT: &str = "The backdrop image could not be shown";
const NOT_COPIED_TEXT: &str = "The backdrop image could not be copied";
const NOT_REMOVED_TEXT: &str = "The old backdrop image could not be removed";

#[derive(Debug)]
enum BackdropFailure {
    Read(io::Error),
    Decode(ImageError),
}

impl fmt::Display for BackdropFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(f, "{error}"),
            Self::Decode(error) => write!(f, "{error}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Decoding {
    file: PathBuf,
    blur: BackdropPercent,
}

impl Decoding {
    fn of(image: &BackdropImage) -> Self {
        Self {
            file: image.file.clone(),
            blur: image.blur,
        }
    }

    fn serves(&self, image: &BackdropImage) -> bool {
        self.file == image.file && self.blur == image.blur
    }
}

#[derive(Default)]
struct Slot {
    wanted: Option<Decoding>,
    shown: Option<(Decoding, Arc<RenderImage>)>,
    retired: Vec<Arc<RenderImage>>,
}

#[derive(Default)]
pub(super) struct Backdrop {
    pub(super) material: Option<(WindowBackgroundAppearance, bool)>,
    window: Slot,
    chat: Slot,
    viewport: Option<Viewport>,
}

impl Backdrop {
    const fn slot(&self, place: BackdropPlace) -> &Slot {
        match place {
            BackdropPlace::Window => &self.window,
            BackdropPlace::Chat => &self.chat,
        }
    }

    const fn slot_mut(&mut self, place: BackdropPlace) -> &mut Slot {
        match place {
            BackdropPlace::Window => &mut self.window,
            BackdropPlace::Chat => &mut self.chat,
        }
    }
}

impl Slot {
    fn wants(&self, image: Option<&BackdropImage>) -> bool {
        match (&self.wanted, image) {
            (None, None) => false,
            (Some(wanted), Some(image)) => !wanted.serves(image),
            _ => true,
        }
    }

    fn retire_shown(&mut self) {
        if let Some((_, image)) = self.shown.take() {
            self.retired.push(image);
        }
    }
}

fn blur_sigma(blur: BackdropPercent) -> f32 {
    blur.fraction() * MAX_BLUR_SIGMA
}

fn fitted(decoded: DynamicImage) -> DynamicImage {
    if decoded.width() > MAX_EDGE || decoded.height() > MAX_EDGE {
        decoded.thumbnail(MAX_EDGE, MAX_EDGE)
    } else {
        decoded
    }
}

fn as_bgra(mut pixels: RgbaImage) -> RgbaImage {
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    pixels
}

fn decode(bytes: &[u8], blur: BackdropPercent) -> Result<RgbaImage, ImageError> {
    let rgba = fitted(image::load_from_memory(bytes)?).to_rgba8();
    let sigma = blur_sigma(blur);
    let softened = if sigma > 0.0 {
        fast_blur(&rgba, sigma)
    } else {
        rgba
    };
    Ok(as_bgra(softened))
}

#[tracing::instrument(skip_all)]
fn load(file: &Path, blur: BackdropPercent) -> Result<Arc<RenderImage>, BackdropFailure> {
    let bytes = std::fs::read(file).map_err(BackdropFailure::Read)?;
    let pixels = decode(&bytes, blur).map_err(BackdropFailure::Decode)?;
    Ok(Arc::new(RenderImage::new(vec![Frame::new(pixels)])))
}

impl EnvironmentShell {
    pub(super) fn sync_backdrop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.backdrop.viewport = Some(Viewport::of(window));
        for place in BackdropPlace::ALL {
            self.sync_backdrop_slot(place, window, cx);
        }
    }

    fn sync_backdrop_slot(
        &mut self,
        place: BackdropPlace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let slot = self.backdrop.slot_mut(place);
        for image in slot.retired.drain(..) {
            window.drop_image(image).unwrap_or_default();
        }
        let chosen = self.settings.backdrop(place);
        if !slot.wants(chosen) {
            return;
        }
        let Some(chosen) = chosen else {
            slot.wanted = None;
            slot.retire_shown();
            return;
        };
        let decoding = Decoding::of(chosen);
        slot.wanted = Some(decoding.clone());
        let loading = cx.background_spawn({
            let decoding = decoding.clone();
            async move { load(&decoding.file, decoding.blur) }
        });
        cx.spawn(async move |this, cx| {
            let loaded = loading.await;
            this.update(cx, |shell, cx| {
                shell.show_backdrop(place, decoding, loaded, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn show_backdrop(
        &mut self,
        place: BackdropPlace,
        decoding: Decoding,
        loaded: Result<Arc<RenderImage>, BackdropFailure>,
        cx: &mut Context<Self>,
    ) {
        let slot = self.backdrop.slot_mut(place);
        if slot.wanted.as_ref() != Some(&decoding) {
            if let Ok(image) = loaded {
                slot.retired.push(image);
            }
            return;
        }
        match loaded {
            Ok(image) => {
                slot.retire_shown();
                slot.shown = Some((decoding, image));
            }
            Err(error) => {
                tracing::warn!(%error, ?place, "backdrop image not shown");
                slot.wanted = None;
                slot.retire_shown();
                self.push_toast(ToastKind::Error, format!("{NOT_SHOWN_TEXT}: {error}"));
                *self.settings.backdrop_mut(place) = None;
                self.save_settings(cx);
            }
        }
        cx.notify();
    }

    pub(super) fn backdrop_layer(
        &self,
        place: BackdropPlace,
        cx: &Context<Self>,
    ) -> Option<BackdropLayer> {
        if transparency_reduced() {
            return None;
        }
        let chosen = self.settings.backdrop(place)?;
        let image = self.backdrop_picture(place)?;
        Some(BackdropLayer {
            image: Arc::clone(image),
            fit: chosen.fit,
            opacity: chosen.opacity.fraction(),
            veil: backdrop_veil(palette(cx), chosen.dim),
        })
    }

    pub(in crate::environment_shell) fn chat_backdrop(
        &self,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let viewport = self.backdrop.viewport?;
        let layer = self.backdrop_layer(BackdropPlace::Chat, cx)?;
        Some(backdrop_layer(&layer, viewport))
    }

    pub(super) fn backdrop_picture(&self, place: BackdropPlace) -> Option<&Arc<RenderImage>> {
        self.settings.backdrop(place)?;
        self.backdrop
            .slot(place)
            .shown
            .as_ref()
            .map(|(_, image)| image)
    }

    pub(super) fn pick_backdrop_image(place: BackdropPlace, cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(PICK_PROMPT)),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = answer.await else {
                return;
            };
            let Some(source) = paths.into_iter().next() else {
                return;
            };
            let copied = cx
                .background_spawn(async move { import_backdrop_image(place, &source) })
                .await;
            this.update(cx, |shell, cx| match copied {
                Ok(copy) => shell.use_backdrop_copy(place, &copy, cx),
                Err(error) => {
                    tracing::warn!(%error, ?place, "backdrop image not copied");
                    shell.push_toast(ToastKind::Error, format!("{NOT_COPIED_TEXT}: {error}"));
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn use_backdrop_copy(&mut self, place: BackdropPlace, copy: &Path, cx: &mut Context<Self>) {
        let chosen = self.settings.backdrop_mut(place);
        let image = match chosen.take() {
            Some(previous) => BackdropImage {
                file: copy.to_path_buf(),
                ..previous
            },
            None => BackdropImage::new(copy),
        };
        *chosen = Some(image);
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn clear_backdrop_image(&mut self, place: BackdropPlace, cx: &mut Context<Self>) {
        let Some(previous) = self.settings.backdrop_mut(place).take() else {
            return;
        };
        self.save_settings(cx);
        let removing =
            cx.background_spawn(async move { forget_backdrop_image(place, &previous.file) });
        cx.spawn(async move |this, cx| {
            let Err(error) = removing.await else {
                return;
            };
            tracing::warn!(%error, ?place, "backdrop image not removed");
            this.update(cx, |shell, cx| {
                shell.push_toast(ToastKind::Error, format!("{NOT_REMOVED_TEXT}: {error}"));
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn edit_backdrop_image(
        &mut self,
        place: BackdropPlace,
        edit: impl FnOnce(&mut BackdropImage),
        cx: &mut Context<Self>,
    ) {
        let Some(image) = self.settings.backdrop_mut(place).as_mut() else {
            return;
        };
        edit(image);
        self.save_settings(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba};
    use std::io::Cursor;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let picture = RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, 255]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(picture)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap_or_else(|_| unreachable!("a small picture encodes"));
        bytes.into_inner()
    }

    #[test]
    fn a_decoded_image_is_handed_over_as_bgra() {
        let pixels = decode(&png(2, 2), BackdropPercent::new(0))
            .unwrap_or_else(|_| unreachable!("the picture decodes"));
        assert_eq!(pixels.get_pixel(0, 0).0, [30, 20, 10, 255]);
    }

    #[test]
    fn a_huge_image_is_scaled_down_to_the_largest_edge() {
        let pixels = decode(&png(MAX_EDGE * 2, 10), BackdropPercent::new(0))
            .unwrap_or_else(|_| unreachable!("the picture decodes"));
        assert_eq!(pixels.width(), MAX_EDGE);
    }

    #[test]
    fn a_corrupt_file_is_a_decode_failure() {
        assert!(decode(b"not a picture", BackdropPercent::new(0)).is_err());
        assert!(matches!(
            load(Path::new("/nowhere/backdrop.png"), BackdropPercent::new(0)),
            Err(BackdropFailure::Read(_))
        ));
    }

    #[test]
    fn a_new_file_or_blur_asks_for_another_decode_but_opacity_does_not() {
        let image = BackdropImage::new("/config/backdrop/image-1.png");
        let mut backdrop = Slot::default();
        assert!(backdrop.wants(Some(&image)));
        assert!(!backdrop.wants(None));
        backdrop.wanted = Some(Decoding::of(&image));
        assert!(!backdrop.wants(Some(&image)));
        let dimmer = BackdropImage {
            opacity: BackdropPercent::new(10),
            ..image.clone()
        };
        assert!(!backdrop.wants(Some(&dimmer)));
        let blurred = BackdropImage {
            blur: BackdropPercent::new(40),
            ..image
        };
        assert!(backdrop.wants(Some(&blurred)));
        assert!(backdrop.wants(None));
    }
}
