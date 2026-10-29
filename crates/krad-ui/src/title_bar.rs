use gpui_kit::component::{InteractiveElementExt as _, TitleBar};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, point, px, AnyElement, App, Decorations, InteractiveElement, IntoElement, MouseButton,
    ParentElement, RenderOnce, SharedString, Styled, TitlebarOptions, Window, WindowControlArea,
};

use crate::theme::WINDOW_INSET;
use crate::traffic_lights;

pub(crate) const TITLE_BAR_HEIGHT_PX: f32 = crate::theme::TOOLBAR_H;
const NATIVE_LIGHT_SIZE_PX: f32 = 16.0;
const NATIVE_LIGHT_STEP_PX: f32 = 23.0;
const NATIVE_LIGHT_COUNT: f32 = 3.0;
const NATIVE_LIGHTS_SHIFT_PX: f32 = 1.0;
const NATIVE_LIGHTS_MARGIN_PX: f32 =
    WINDOW_INSET + (TITLE_BAR_HEIGHT_PX - NATIVE_LIGHT_SIZE_PX) / 2.0;
#[cfg(target_os = "macos")]
pub(crate) const LEADING_CONTROLS_END_PX: f32 = NATIVE_LIGHTS_MARGIN_PX
    + NATIVE_LIGHT_STEP_PX * (NATIVE_LIGHT_COUNT - 1.0)
    + NATIVE_LIGHT_SIZE_PX;
#[cfg(not(target_os = "macos"))]
pub(crate) const LEADING_CONTROLS_END_PX: f32 = WINDOW_INSET;
const EDGE_PADDING_PX: f32 = 12.0;
const CONTROLS_GAP_PX: f32 = 12.0;
const DRAG_REGION_SUFFIX: &str = "-drag";
const IS_LINUX: bool = cfg!(target_os = "linux");
const IS_MACOS: bool = cfg!(target_os = "macos");

#[derive(IntoElement)]
pub(crate) struct WindowTitleBar {
    id: &'static str,
    children: Vec<AnyElement>,
    controls: bool,
    leading: f32,
    padding: f32,
    gap: f32,
}

impl WindowTitleBar {
    pub(crate) const fn new(id: &'static str) -> Self {
        Self {
            id,
            children: Vec::new(),
            controls: true,
            leading: EDGE_PADDING_PX,
            padding: EDGE_PADDING_PX,
            gap: CONTROLS_GAP_PX,
        }
    }

    pub(crate) const fn without_controls(mut self) -> Self {
        self.controls = false;
        self
    }

    pub(crate) const fn padding(mut self, padding: f32) -> Self {
        self.leading = padding;
        self.padding = padding;
        self
    }

    pub(crate) const fn leading(mut self, leading: f32) -> Self {
        self.leading = leading;
        self
    }

    pub(crate) const fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

impl ParentElement for WindowTitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

struct MovePending(bool);

impl RenderOnce for WindowTitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pending = window.use_keyed_state(self.id, cx, |_, _| MovePending(false));
        let lights = self.controls.then(|| traffic_lights::render(window));
        let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        div()
            .id(self.id)
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(self.gap))
            .h(px(TITLE_BAR_HEIGHT_PX))
            .pl(px(self.leading))
            .pr(px(self.padding))
            .on_double_click(|_, window, _| zoom_on_double_click(window))
            .when(IS_LINUX && client_decorated, |bar| {
                bar.on_mouse_down(MouseButton::Right, |event, window, _| {
                    window.show_window_menu(event.position);
                })
            })
            .on_mouse_down_out(window.listener_for(&pending, |state, _, _, _| state.0 = false))
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&pending, |state, _, _, _| state.0 = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                window.listener_for(&pending, |state, _, _, _| state.0 = false),
            )
            .on_mouse_move(window.listener_for(&pending, |state, _, window, _| {
                if state.0 {
                    state.0 = false;
                    window.start_window_move();
                }
            }))
            .child(
                div()
                    .id(SharedString::from(format!(
                        "{}{DRAG_REGION_SUFFIX}",
                        self.id
                    )))
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .justify_between()
                    .window_control_area(WindowControlArea::Drag)
                    .children(self.children),
            )
            .children(lights)
    }
}

fn zoom_on_double_click(window: &mut Window) {
    if IS_MACOS {
        window.titlebar_double_click();
    } else if IS_LINUX {
        window.zoom_window();
    }
}

#[must_use]
pub fn title_bar_options() -> TitlebarOptions {
    let origin = px(NATIVE_LIGHTS_MARGIN_PX + NATIVE_LIGHTS_SHIFT_PX);
    TitlebarOptions {
        traffic_light_position: Some(point(origin, origin)),
        ..TitleBar::title_bar_options()
    }
}
