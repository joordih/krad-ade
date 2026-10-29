use gpui_kit::component::{window_border, window_paddings};
use gpui_kit::{
    div, px, size, AnyElement, App, Background, Bounds, CursorStyle, Decorations,
    InteractiveElement, IntoElement, MouseButton, ParentElement, ResizeEdge, Styled, Window,
    WindowBackgroundAppearance, WindowBounds,
};

use krad_domain::WindowMaterial;
use std::sync::OnceLock;

use crate::theme::Palette;

const INITIAL_WIDTH: f32 = 1440.0;
const INITIAL_HEIGHT: f32 = 900.0;
const MAX_DISPLAY_SHARE: f32 = 0.85;

const WINDOW_RADIUS: f32 = 10.0;
const WINDOW_SHADOW_PX: f32 = 0.0;
const RESIZE_BAND: f32 = 6.0;
const RESIZE_CORNER: f32 = 14.0;
const IS_LINUX: bool = cfg!(target_os = "linux");
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

fn system_has_mica() -> bool {
    static MICA: OnceLock<bool> = OnceLock::new();
    #[cfg(windows)]
    let read = windows::has_mica;
    #[cfg(not(windows))]
    let read = || false;
    *MICA.get_or_init(read)
}

fn system_has_glass() -> bool {
    static GLASS: OnceLock<bool> = OnceLock::new();
    #[cfg(target_os = "macos")]
    let read = macos::has_glass;
    #[cfg(not(target_os = "macos"))]
    let read = || false;
    *GLASS.get_or_init(read)
}

#[must_use]
pub(crate) fn usable_material(material: WindowMaterial) -> WindowMaterial {
    match material {
        WindowMaterial::Opaque => WindowMaterial::Opaque,
        _ if !system_transparency() => WindowMaterial::Opaque,
        WindowMaterial::Mica if !system_has_mica() => WindowMaterial::Blurred,
        WindowMaterial::Glass if !system_has_glass() => WindowMaterial::Blurred,
        material => material,
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
    pal: &Palette,
    backdrop: Background,
) -> AnyElement {
    let floating = match window.window_decorations() {
        Decorations::Client { tiling } => IS_LINUX && !tiling.is_tiled(),
        Decorations::Server => false,
    };
    if !floating {
        return window_border()
            .shadow_size(px(WINDOW_SHADOW_PX))
            .child(div().size_full().bg(backdrop).child(content))
            .into_any_element();
    }
    div()
        .relative()
        .size_full()
        .rounded(px(WINDOW_RADIUS))
        .border_1()
        .border_color(pal.border)
        .bg(backdrop)
        .child(content)
        .children(resize_zones())
        .into_any_element()
}

fn resize_zones() -> Vec<AnyElement> {
    let edge = |resize: ResizeEdge, cursor: CursorStyle| {
        div()
            .absolute()
            .cursor(cursor)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                window.start_window_resize(resize);
            })
    };
    vec![
        edge(ResizeEdge::Top, CursorStyle::ResizeUpDown)
            .top_0()
            .left(px(RESIZE_CORNER))
            .right(px(RESIZE_CORNER))
            .h(px(RESIZE_BAND)),
        edge(ResizeEdge::Bottom, CursorStyle::ResizeUpDown)
            .bottom_0()
            .left(px(RESIZE_CORNER))
            .right(px(RESIZE_CORNER))
            .h(px(RESIZE_BAND)),
        edge(ResizeEdge::Left, CursorStyle::ResizeLeftRight)
            .left_0()
            .top(px(RESIZE_CORNER))
            .bottom(px(RESIZE_CORNER))
            .w(px(RESIZE_BAND)),
        edge(ResizeEdge::Right, CursorStyle::ResizeLeftRight)
            .right_0()
            .top(px(RESIZE_CORNER))
            .bottom(px(RESIZE_CORNER))
            .w(px(RESIZE_BAND)),
        edge(ResizeEdge::TopLeft, CursorStyle::ResizeUpLeftDownRight)
            .top_0()
            .left_0()
            .size(px(RESIZE_CORNER)),
        edge(ResizeEdge::BottomRight, CursorStyle::ResizeUpLeftDownRight)
            .bottom_0()
            .right_0()
            .size(px(RESIZE_CORNER)),
        edge(ResizeEdge::TopRight, CursorStyle::ResizeUpRightDownLeft)
            .top_0()
            .right_0()
            .size(px(RESIZE_CORNER)),
        edge(ResizeEdge::BottomLeft, CursorStyle::ResizeUpRightDownLeft)
            .bottom_0()
            .left_0()
            .size(px(RESIZE_CORNER)),
    ]
    .into_iter()
    .map(IntoElement::into_any_element)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_is_a_transparent_window_where_the_system_has_it_and_a_blur_elsewhere() {
        let expected = match (system_transparency(), system_has_glass()) {
            (false, _) if IS_LINUX => WindowBackgroundAppearance::Transparent,
            (false, _) => WindowBackgroundAppearance::Opaque,
            (true, true) => WindowBackgroundAppearance::Transparent,
            (true, false) => WindowBackgroundAppearance::Blurred,
        };
        assert_eq!(window_background(WindowMaterial::Glass, true), expected);
        assert_eq!(window_background(WindowMaterial::Glass, false), expected);
    }
}
