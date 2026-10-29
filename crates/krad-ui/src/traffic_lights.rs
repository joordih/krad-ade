use gpui_kit::prelude::FluentBuilder;
use gpui_kit::App;
use gpui_kit::{
    div, px, rgb, AnyElement, Decorations, InteractiveElement, IntoElement, MouseButton,
    ParentElement, StatefulInteractiveElement, Styled, Window, WindowControlArea,
};

use crate::actions::CloseWindow;

const NATIVE_LIGHTS: bool = cfg!(target_os = "macos");
const HIT_TEST_CONTROLS: bool = cfg!(target_os = "windows");
const SERVER_DECORATIONS_POSSIBLE: bool = cfg!(target_os = "linux");
const LIGHT_ORDER: [Light; 3] = [Light::Minimize, Light::Zoom, Light::Close];
const LIGHT_SIZE: f32 = 12.0;
pub(crate) const CLOSE_COLOR: u32 = 0x00ff_5f56;
pub(crate) const MINIMIZE_COLOR: u32 = 0x00ff_bd2e;
pub(crate) const ZOOM_COLOR: u32 = 0x0027_c93f;

#[derive(Clone, Copy)]
enum Light {
    Close,
    Minimize,
    Zoom,
}

impl Light {
    const fn id(self) -> &'static str {
        match self {
            Self::Close => "traffic-light-close",
            Self::Minimize => "traffic-light-minimize",
            Self::Zoom => "traffic-light-zoom",
        }
    }

    const fn color(self) -> u32 {
        match self {
            Self::Close => CLOSE_COLOR,
            Self::Minimize => MINIMIZE_COLOR,
            Self::Zoom => ZOOM_COLOR,
        }
    }

    const fn area(self) -> WindowControlArea {
        match self {
            Self::Close => WindowControlArea::Close,
            Self::Minimize => WindowControlArea::Min,
            Self::Zoom => WindowControlArea::Max,
        }
    }

    fn trigger(self, window: &mut Window, cx: &mut App) {
        match self {
            Self::Close => window.dispatch_action(Box::new(CloseWindow), cx),
            Self::Minimize => window.minimize_window(),
            Self::Zoom => window.zoom_window(),
        }
    }
}

fn light(kind: Light) -> AnyElement {
    let base = div()
        .id(kind.id())
        .flex_none()
        .size(px(LIGHT_SIZE))
        .rounded_full()
        .bg(rgb(kind.color()));
    if HIT_TEST_CONTROLS {
        base.window_control_area(kind.area()).into_any_element()
    } else {
        base.on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            kind.trigger(window, cx);
        })
        .into_any_element()
    }
}

pub(crate) fn render(window: &Window) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap_2()
        .when(!system_draws_controls(window), |row| {
            row.children(LIGHT_ORDER.map(light))
        })
        .into_any_element()
}

fn system_draws_controls(window: &Window) -> bool {
    NATIVE_LIGHTS
        || (SERVER_DECORATIONS_POSSIBLE
            && !matches!(window.window_decorations(), Decorations::Client { .. }))
}
