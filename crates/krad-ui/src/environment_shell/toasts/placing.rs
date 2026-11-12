use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    canvas, div, fill, point, px, rgb, size, AnyElement, App, Bounds, Context, CursorStyle,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement,
    Rgba, SharedString, Styled,
};
use krad_domain::{ToastKind, WindowSide};

use super::snap::{Anchor, Frame, Guides, Snapping, GRID};
use super::{kind_icon, toast_column};
use crate::components::floating::floating_surface;
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::environment_shell::EnvironmentShell;
use crate::keymap::FREE_DROP_HINT;
use crate::theme::{
    dialog_scrim, palette, Palette, ICON_SM, LINE_SM, SPACE_1, SPACE_2, SPACE_3, TEXT_SM,
};

const SAMPLE_LEAD: &str = "Drag to place toasts. Hold";
const SAMPLE_TAIL: &str = "to drop freely.";
const DONE_LABEL: &str = "Done";
const CANCEL_LABEL: &str = "Cancel";
const RESET_LABEL: &str = "Reset";
const BTN_DONE: &str = "toast-placing-done";
const BTN_CANCEL: &str = "toast-placing-cancel";
const BTN_RESET: &str = "toast-placing-reset";
const BAR_EDGE: f32 = 64.0;
const GUIDE_W: f32 = 1.0;
const GRID_LINE_W: f32 = 1.0;
const GRID_ALPHA: f32 = 0.06;
const GUIDE_ALPHA: f32 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Grab {
    pointer: Anchor,
    anchor: Anchor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ToastPlacing {
    anchor: Option<Anchor>,
    grab: Option<Grab>,
    guides: Guides,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn start_toast_placing(&mut self, cx: &mut Context<Self>) {
        self.toasts.placing = Some(ToastPlacing {
            anchor: None,
            grab: None,
            guides: Guides::default(),
        });
        cx.notify();
    }

    fn finish_toast_placing(&mut self, frame: Frame, keep: bool, cx: &mut Context<Self>) {
        let Some(placing) = self.toasts.placing.take() else {
            return;
        };
        if keep {
            if let Some(anchor) = placing.anchor {
                self.settings.toasts.placement = Some(frame.placement_of(anchor));
                self.save_settings(cx);
            }
        }
        cx.notify();
    }

    fn reset_toast_placing(&mut self, cx: &mut Context<Self>) {
        self.toasts.placing = None;
        self.settings.toasts.placement = None;
        self.save_settings(cx);
        cx.notify();
    }

    fn grab_toast_sample(&mut self, frame: Frame, event: &MouseDownEvent) {
        let current = self.placing_anchor(frame);
        let Some(placing) = self.toasts.placing.as_mut() else {
            return;
        };
        placing.grab = Some(Grab {
            pointer: Anchor {
                x: event.position.x.as_f32(),
                y: event.position.y.as_f32(),
            },
            anchor: current,
        });
    }

    fn drag_toast_sample(&mut self, frame: Frame, event: &MouseMoveEvent) -> bool {
        let Some(placing) = self.toasts.placing.as_mut() else {
            return false;
        };
        let Some(grab) = placing.grab else {
            return false;
        };
        let moved = Anchor {
            x: grab.anchor.x + event.position.x.as_f32() - grab.pointer.x,
            y: grab.anchor.y + event.position.y.as_f32() - grab.pointer.y,
        };
        let snapping = if event.modifiers.alt {
            Snapping::Off
        } else {
            Snapping::On
        };
        let (anchor, guides) = frame.snap(moved, snapping);
        placing.anchor = Some(anchor);
        placing.guides = guides;
        true
    }

    fn drop_toast_sample(&mut self) {
        if let Some(placing) = self.toasts.placing.as_mut() {
            placing.grab = None;
            placing.guides = Guides::default();
        }
    }

    fn placing_anchor(&self, frame: Frame) -> Anchor {
        self.toasts
            .placing
            .and_then(|placing| placing.anchor)
            .unwrap_or_else(|| frame.anchor_of(self.settings.toasts.placement))
    }

    pub(in crate::environment_shell) fn toast_placing_overlay(
        &self,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let placing = self.toasts.placing?;
        let pal = palette(cx);
        let frame = self.toast_frame(width, height);
        let anchor = self.placing_anchor(frame);
        let side = frame.side_of(anchor);
        let dragging = placing.grab.is_some();
        let sample = sample_card(side, pal, cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                    this.grab_toast_sample(frame, event);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element();
        Some(
            div()
                .id(SharedString::from("toast-placing"))
                .absolute()
                .inset_0()
                .occlude()
                .bg(dialog_scrim(pal, cx))
                .on_mouse_move(cx.listener(
                    move |this: &mut Self, event: &MouseMoveEvent, _, cx| {
                        if this.drag_toast_sample(frame, event) {
                            cx.notify();
                        }
                    },
                ))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.drop_toast_sample();
                        cx.notify();
                    }),
                )
                .when(dragging, |layer| {
                    layer.child(grid_and_guides(placing.guides, pal.accent))
                })
                .child(toast_column(frame, anchor, vec![sample]))
                .child(Self::placing_bar(frame, anchor, cx))
                .into_any_element(),
        )
    }

    fn placing_bar(frame: Frame, anchor: Anchor, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let below = anchor.y < frame.height / 2.0;
        let buttons = [
            Button::new(BTN_RESET)
                .label(RESET_LABEL)
                .variant(ButtonVariant::Ghost)
                .small()
                .on_click(cx.listener(|this: &mut Self, _, _, cx| this.reset_toast_placing(cx))),
            Button::new(BTN_CANCEL)
                .label(CANCEL_LABEL)
                .variant(ButtonVariant::Secondary)
                .small()
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.finish_toast_placing(frame, false, cx);
                })),
            Button::new(BTN_DONE)
                .label(DONE_LABEL)
                .variant(ButtonVariant::Primary)
                .small()
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.finish_toast_placing(frame, true, cx);
                })),
        ];
        div()
            .absolute()
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .map(|row| {
                if below {
                    row.bottom(px(BAR_EDGE))
                } else {
                    row.top(px(BAR_EDGE))
                }
            })
            .child(
                floating_surface(pal, cx)
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .p(px(SPACE_1))
                    .children(buttons),
            )
            .into_any_element()
    }
}

fn sample_card(side: WindowSide, pal: &Palette, cx: &App) -> gpui_kit::Div {
    let (glyph, tint) = kind_icon(ToastKind::Info, pal);
    let mut parts = vec![
        div()
            .flex_none()
            .child(icon(glyph, ICON_SM, tint))
            .into_any_element(),
        div()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_default))
            .child(format!("{SAMPLE_LEAD} {FREE_DROP_HINT} {SAMPLE_TAIL}"))
            .into_any_element(),
        div()
            .flex_none()
            .child(icon(IconName::Move, ICON_SM, pal.text_muted))
            .into_any_element(),
    ];
    if side == WindowSide::Left {
        parts.reverse();
    }
    floating_surface(pal, cx)
        .flex_row()
        .items_center()
        .gap(px(SPACE_2))
        .px(px(SPACE_3))
        .py(px(SPACE_2))
        .cursor(CursorStyle::ClosedHand)
        .children(parts)
}

fn grid_and_guides(guides: Guides, accent: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<gpui_kit::Pixels>, (), window, _: &mut App| {
            let line = rgb(accent);
            let grid = Rgba {
                a: GRID_ALPHA,
                ..line
            };
            let guide = Rgba {
                a: GUIDE_ALPHA,
                ..line
            };
            let width = bounds.size.width.as_f32();
            let height = bounds.size.height.as_f32();
            let origin = bounds.origin;
            let mut x = 0.0;
            while x < width {
                window.paint_quad(fill(
                    Bounds::new(
                        origin + point(px(x), px(0.0)),
                        size(px(GRID_LINE_W), px(height)),
                    ),
                    grid,
                ));
                x += GRID;
            }
            let mut y = 0.0;
            while y < height {
                window.paint_quad(fill(
                    Bounds::new(
                        origin + point(px(0.0), px(y)),
                        size(px(width), px(GRID_LINE_W)),
                    ),
                    grid,
                ));
                y += GRID;
            }
            if let Some(x) = guides.x {
                window.paint_quad(fill(
                    Bounds::new(
                        origin + point(px(x), px(0.0)),
                        size(px(GUIDE_W), px(height)),
                    ),
                    guide,
                ));
            }
            if let Some(y) = guides.y {
                window.paint_quad(fill(
                    Bounds::new(origin + point(px(0.0), px(y)), size(px(width), px(GUIDE_W))),
                    guide,
                ));
            }
        },
    )
    .absolute()
    .inset_0()
}
