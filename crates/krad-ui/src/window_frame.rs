use gpui_kit::component::{window_border, window_paddings};
use gpui_kit::{
    div, px, size, AnyElement, App, Background, Bounds, Decorations, IntoElement, ParentElement,
    Styled, Window, WindowBackgroundAppearance, WindowBounds,
};

use krad_domain::WindowMaterial;
use std::sync::OnceLock;

pub(crate) use backdrop::{backdrop_layer, BackdropLayer, Viewport};

const INITIAL_WIDTH: f32 = 1440.0;
const INITIAL_HEIGHT: f32 = 900.0;
const MAX_DISPLAY_SHARE: f32 = 0.85;

const WINDOW_SHADOW_PX: f32 = 0.0;
const IS_LINUX: bool = cfg!(target_os = "linux");
const LINUX_WINDOW_RADIUS: f32 = 10.0;
pub(crate) const CLIENT_WINDOW_RADIUS: f32 = if IS_LINUX { LINUX_WINDOW_RADIUS } else { 0.0 };
const IS_MACOS: bool = cfg!(target_os = "macos");
const OFFERED_MATERIALS: &[WindowMaterial] = if IS_MACOS {
    &[
        WindowMaterial::Opaque,
        WindowMaterial::Blurred,
        WindowMaterial::Glass,
    ]
} else {
    &[
        WindowMaterial::Opaque,
        WindowMaterial::Blurred,
        WindowMaterial::Mica,
    ]
};
const ROOT_BORDER: f32 = 1.0;
const FRAME_BORDER: f32 = 1.0;

pub(crate) fn content_inset(window: &Window) -> (f32, f32) {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return (0.0, 0.0);
    };
    let paddings = window_paddings(window);
    let side = |shadow: f32, tiled: bool| {
        if tiled {
            shadow
        } else {
            shadow + ROOT_BORDER + FRAME_BORDER
        }
    };
    (
        side(paddings.left.as_f32(), tiling.left),
        side(paddings.top.as_f32(), tiling.top),
    )
}

#[must_use]
pub fn initial_window_bounds(cx: &App) -> WindowBounds {
    let display = cx.primary_display().map(|display| display.bounds().size);
    let fit = |wanted: f32, available: Option<f32>| {
        available.map_or(wanted, |available| {
            wanted.min(available * MAX_DISPLAY_SHARE)
        })
    };
    let width = fit(INITIAL_WIDTH, display.map(|size| size.width.as_f32()));
    let height = fit(INITIAL_HEIGHT, display.map(|size| size.height.as_f32()));
    WindowBounds::Windowed(Bounds::centered(None, size(px(width), px(height)), cx))
}

mod backdrop;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
fn system_transparency() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(windows::transparency_enabled)
}

#[cfg(target_os = "macos")]
fn system_transparency() -> bool {
    !macos::reduces_transparency()
}

#[cfg(not(any(windows, target_os = "macos")))]
const fn system_transparency() -> bool {
    true
}

#[must_use]
pub(crate) fn transparency_reduced() -> bool {
    !system_transparency()
}

fn system_has_mica() -> bool {
    static MICA: OnceLock<bool> = OnceLock::new();
    #[cfg(windows)]
    let read = windows::has_mica;
    #[cfg(not(windows))]
    let read = || false;
    *MICA.get_or_init(read)
}

fn system_has_blur() -> bool {
    static BLUR: OnceLock<bool> = OnceLock::new();
    #[cfg(target_os = "linux")]
    let read = linux::compositor_blurs;
    #[cfg(not(target_os = "linux"))]
    let read = || true;
    *BLUR.get_or_init(read)
}

fn system_has_glass() -> bool {
    static GLASS: OnceLock<bool> = OnceLock::new();
    #[cfg(target_os = "macos")]
    let read = macos::has_glass;
    #[cfg(not(target_os = "macos"))]
    let read = || false;
    *GLASS.get_or_init(read)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SystemMaterials {
    transparency: bool,
    blur: bool,
    native: NativeMaterials,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NativeMaterials {
    mica: bool,
    glass: bool,
}

#[must_use]
pub(crate) fn usable_material(material: WindowMaterial) -> WindowMaterial {
    let system = SystemMaterials {
        transparency: system_transparency(),
        blur: system_has_blur(),
        native: NativeMaterials {
            mica: system_has_mica(),
            glass: system_has_glass(),
        },
    };
    material_on(material, system)
}

const fn material_on(material: WindowMaterial, system: SystemMaterials) -> WindowMaterial {
    match material {
        WindowMaterial::Opaque => WindowMaterial::Opaque,
        _ if !system.transparency => WindowMaterial::Opaque,
        WindowMaterial::Mica if system.native.mica => WindowMaterial::Mica,
        WindowMaterial::Glass if system.native.glass => WindowMaterial::Glass,
        _ if !system.blur => WindowMaterial::Opaque,
        _ => WindowMaterial::Blurred,
    }
}

pub(crate) const fn offered_materials() -> &'static [WindowMaterial] {
    OFFERED_MATERIALS
}

pub(crate) const fn tint_adjustable(material: WindowMaterial) -> bool {
    IS_MACOS && matches!(material, WindowMaterial::Blurred | WindowMaterial::Glass)
}

#[cfg(windows)]
pub fn match_window_frame(dark: bool) {
    windows::match_frame_to(dark);
}

#[cfg(not(windows))]
pub const fn match_window_frame(_dark: bool) {}

pub(crate) fn replace_window_background(
    window: &mut Window,
    wanted: WindowBackgroundAppearance,
    dark: bool,
) {
    let _ = dark;
    #[cfg(windows)]
    if windows::apply_background(window, wanted, dark) {
        return;
    }
    window.set_background_appearance(WindowBackgroundAppearance::Opaque);
    window.set_background_appearance(wanted);
    #[cfg(target_os = "macos")]
    macos::apply_glass(window, wanted == WindowBackgroundAppearance::Transparent);
}

#[must_use]
pub fn window_background(material: WindowMaterial, dark: bool) -> WindowBackgroundAppearance {
    usable_window_background(usable_material(material), dark)
}

#[must_use]
pub(crate) const fn usable_window_background(
    usable: WindowMaterial,
    dark: bool,
) -> WindowBackgroundAppearance {
    match usable {
        WindowMaterial::Opaque if IS_LINUX => WindowBackgroundAppearance::Transparent,
        WindowMaterial::Opaque => WindowBackgroundAppearance::Opaque,
        WindowMaterial::Mica if dark => WindowBackgroundAppearance::MicaAltBackdrop,
        WindowMaterial::Mica => WindowBackgroundAppearance::MicaBackdrop,
        WindowMaterial::Blurred => WindowBackgroundAppearance::Blurred,
        WindowMaterial::Glass => WindowBackgroundAppearance::Transparent,
    }
}

pub(crate) fn window_frame(
    content: AnyElement,
    window: &Window,
    (backdrop, image): (Background, Option<BackdropLayer>),
) -> AnyElement {
    let underlay = image.map(|layer| backdrop_layer(&layer, Viewport::of(window)));
    window_border()
        .shadow_size(px(WINDOW_SHADOW_PX))
        .child(
            div()
                .relative()
                .size_full()
                .bg(backdrop)
                .children(underlay)
                .child(content),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_material_the_system_lacks_falls_back_to_blur_or_to_opaque() {
        let full = SystemMaterials {
            transparency: true,
            blur: true,
            native: NativeMaterials {
                mica: true,
                glass: true,
            },
        };
        let bare = SystemMaterials {
            native: NativeMaterials::default(),
            ..full
        };
        let solid = SystemMaterials {
            transparency: false,
            ..full
        };
        let unblurred = SystemMaterials {
            blur: false,
            ..bare
        };
        for (material, system, expected) in [
            (WindowMaterial::Glass, full, WindowMaterial::Glass),
            (WindowMaterial::Mica, full, WindowMaterial::Mica),
            (WindowMaterial::Glass, bare, WindowMaterial::Blurred),
            (WindowMaterial::Mica, bare, WindowMaterial::Blurred),
            (WindowMaterial::Blurred, bare, WindowMaterial::Blurred),
            (WindowMaterial::Glass, solid, WindowMaterial::Opaque),
            (WindowMaterial::Blurred, solid, WindowMaterial::Opaque),
            (WindowMaterial::Opaque, full, WindowMaterial::Opaque),
            (WindowMaterial::Glass, unblurred, WindowMaterial::Opaque),
            (WindowMaterial::Blurred, unblurred, WindowMaterial::Opaque),
            (WindowMaterial::Mica, unblurred, WindowMaterial::Opaque),
        ] {
            assert_eq!(
                material_on(material, system),
                expected,
                "{material:?} on {system:?}"
            );
        }
    }

    #[test]
    fn each_usable_material_asks_the_window_for_its_own_background() {
        let opaque = if cfg!(target_os = "linux") {
            WindowBackgroundAppearance::Transparent
        } else {
            WindowBackgroundAppearance::Opaque
        };
        for (material, dark, expected) in [
            (WindowMaterial::Opaque, false, opaque),
            (WindowMaterial::Opaque, true, opaque),
            (
                WindowMaterial::Mica,
                false,
                WindowBackgroundAppearance::MicaBackdrop,
            ),
            (
                WindowMaterial::Mica,
                true,
                WindowBackgroundAppearance::MicaAltBackdrop,
            ),
            (
                WindowMaterial::Blurred,
                false,
                WindowBackgroundAppearance::Blurred,
            ),
            (
                WindowMaterial::Blurred,
                true,
                WindowBackgroundAppearance::Blurred,
            ),
            (
                WindowMaterial::Glass,
                false,
                WindowBackgroundAppearance::Transparent,
            ),
            (
                WindowMaterial::Glass,
                true,
                WindowBackgroundAppearance::Transparent,
            ),
        ] {
            assert_eq!(
                usable_window_background(material, dark),
                expected,
                "{material:?}, dark {dark}"
            );
        }
    }
}
