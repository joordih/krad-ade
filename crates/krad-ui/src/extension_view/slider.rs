mod labels;
mod motion;

use futures::channel::mpsc::UnboundedSender;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    canvas, div, px, relative, rgb, AnyElement, App, Bounds, Context, DispatchPhase, FocusHandle,
    FontFeatures, Hsla, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, StatefulInteractiveElement,
    Styled, WeakEntity, Window,
};
use krad_extensions::api::view::{Slider, ViewEvent};
use krad_extensions::api::NodeKey;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use super::form::element_id;
use super::ViewOutput;
use crate::theme::{palette, tabular_digits, Palette, LINE_2XS, SPACE_0_5, TEXT_2XS};
use labels::LabelText;
use motion::{KeyMove, SliderMotion};

const SLIDER_PREFIX: &str = "ext-slider";
const HIT_H: f32 = 24.0;
const TRACK_H: f32 = 4.0;
const KNOB_D: f32 = 12.0;
const KNOB_BORDER: f32 = 1.0;
const KEY_LEFT: &str = "left";
const KEY_DOWN: &str = "down";
const KEY_RIGHT: &str = "right";
const KEY_UP: &str = "up";
const KEY_HOME: &str = "home";
const KEY_END: &str = "end";

type TrackArea = Rc<Cell<Option<Bounds<Pixels>>>>;

pub(crate) struct SliderView {
    key: NodeKey,
    motion: SliderMotion,
    labels: LabelText,
    track: TrackArea,
    focus: FocusHandle,
    hovered: bool,
    digits: FontFeatures,
    output: UnboundedSender<ViewOutput>,
}

fn key_move(key: &str) -> Option<KeyMove> {
    match key {
        KEY_LEFT | KEY_DOWN => Some(KeyMove::Back),
        KEY_RIGHT | KEY_UP => Some(KeyMove::Forward),
        KEY_HOME => Some(KeyMove::Start),
        KEY_END => Some(KeyMove::End),
        _ => None,
    }
}

fn fraction_within(bounds: Bounds<Pixels>, position: Point<Pixels>) -> f64 {
    let width = f64::from(f32::from(bounds.size.width));
    if width <= 0.0 {
        return 0.0;
    }
    let offset = f64::from(f32::from(position.x - bounds.origin.x));
    (offset / width).clamp(0.0, 1.0)
}

#[allow(clippy::cast_possible_truncation)]
const fn share(fraction: f64) -> f32 {
    fraction as f32
}

impl SliderView {
    pub(super) fn new(
        key: NodeKey,
        spec: Slider,
        output: UnboundedSender<ViewOutput>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            key,
            motion: SliderMotion::new(spec, Instant::now()),
            labels: LabelText::default(),
            track: Rc::default(),
            focus: cx.focus_handle(),
            hovered: false,
            digits: tabular_digits(),
            output,
        }
    }

    pub(super) fn receive(&mut self, spec: Slider, cx: &mut Context<Self>) {
        self.motion.receive(spec, Instant::now());
        cx.notify();
    }

    fn emit(&self, event: Option<ViewEvent>) {
        let Some(event) = event else {
            return;
        };
        if self
            .output
            .unbounded_send(ViewOutput::Event(event))
            .is_err()
        {
            tracing::trace!("extension slider event dropped after its program stopped");
        }
    }

    fn fraction_at(&self, position: Point<Pixels>) -> Option<f64> {
        self.track
            .get()
            .map(|bounds| fraction_within(bounds, position))
    }

    fn press(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.motion.is_enabled() {
            return;
        }
        window.focus(&self.focus, cx);
        let Some(fraction) = self.fraction_at(event.position) else {
            return;
        };
        let change = self.motion.press(fraction, Instant::now());
        self.emit(change);
        cx.stop_propagation();
        cx.notify();
    }

    fn drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.release(cx);
            return;
        }
        let Some(fraction) = self.fraction_at(event.position) else {
            return;
        };
        let change = self.motion.drag(fraction, Instant::now());
        self.emit(change);
        cx.notify();
    }

    fn release(&mut self, cx: &mut Context<Self>) {
        let commit = self.motion.release();
        if commit.is_some() {
            self.emit(commit);
            cx.notify();
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(step) = key_move(&event.keystroke.key) else {
            return;
        };
        let Some(commit) = self.motion.key(step, Instant::now()) else {
            return;
        };
        self.emit(Some(commit));
        cx.stop_propagation();
        cx.notify();
    }
}

fn knob(fraction: f32, focused: bool, pal: &Palette) -> AnyElement {
    let ring = if focused {
        rgb(pal.accent).into()
    } else {
        Hsla::from(pal.border)
    };
    div()
        .absolute()
        .left(relative(fraction))
        .top(px((TRACK_H - KNOB_D) / 2.0))
        .ml(px(-KNOB_D / 2.0))
        .size(px(KNOB_D))
        .rounded_full()
        .bg(rgb(pal.on_accent))
        .border(px(KNOB_BORDER))
        .border_color(ring)
        .into_any_element()
}

fn track_watch(area: TrackArea, dragging: Option<WeakEntity<SliderView>>) -> AnyElement {
    canvas(
        move |bounds, _, _| area.set(Some(bounds)),
        move |_, (), window, _| {
            let Some(slider) = dragging else {
                return;
            };
            let moved = slider.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx: &mut App| {
                if phase == DispatchPhase::Bubble {
                    moved
                        .update(cx, |slider, cx| slider.drag(event, cx))
                        .unwrap_or_default();
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx: &mut App| {
                if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                    slider.update(cx, SliderView::release).unwrap_or_default();
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

impl Render for SliderView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let pal = palette(cx);
        self.labels
            .update(&self.motion.spec, self.motion.value_at(now));
        if self.motion.is_advancing(now) {
            window.request_animation_frame();
        }
        let enabled = self.motion.is_enabled();
        let dragging = self.motion.is_dragging();
        let focused = self.focus.is_focused(window);
        let shows_knob = enabled && (self.hovered || dragging || focused);
        let (fill, text) = if enabled {
            (pal.accent, pal.text_muted)
        } else {
            (pal.text_disabled, pal.text_disabled)
        };
        let fraction = share(self.motion.fraction_at(now));
        let watcher = dragging.then(|| cx.entity().downgrade());
        let track = div()
            .relative()
            .w_full()
            .h(px(TRACK_H))
            .rounded_full()
            .bg(Hsla::from(pal.border))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(relative(fraction))
                    .rounded_full()
                    .bg(rgb(fill)),
            )
            .when(shows_knob, |track| {
                track.child(knob(fraction, focused, pal))
            });
        let hit = div()
            .id(element_id(SLIDER_PREFIX, &self.key))
            .track_focus(&self.focus)
            .relative()
            .w_full()
            .h(px(HIT_H))
            .flex()
            .items_center()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.hovered = *hovered;
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.press(event, window, cx);
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.key_down(event, cx);
            }))
            .child(track)
            .child(track_watch(self.track.clone(), watcher));
        let has_labels = self.labels.start.is_some() || self.labels.end.is_some();
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .child(hit)
            .when(has_labels, |column| {
                column.child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .mt(px(-SPACE_0_5))
                        .text_size(px(TEXT_2XS))
                        .line_height(px(LINE_2XS))
                        .text_color(rgb(text))
                        .font_features(self.digits.clone())
                        .child(div().children(self.labels.start.clone()))
                        .child(div().children(self.labels.end.clone())),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, size};

    #[test]
    fn keys_map_to_slider_moves() {
        assert_eq!(key_move(KEY_RIGHT), Some(KeyMove::Forward));
        assert_eq!(key_move(KEY_UP), Some(KeyMove::Forward));
        assert_eq!(key_move(KEY_LEFT), Some(KeyMove::Back));
        assert_eq!(key_move(KEY_DOWN), Some(KeyMove::Back));
        assert_eq!(key_move(KEY_HOME), Some(KeyMove::Start));
        assert_eq!(key_move(KEY_END), Some(KeyMove::End));
        assert_eq!(key_move("a"), None);
    }

    #[test]
    fn a_pointer_reads_as_its_share_of_the_track_clamped_to_the_ends() {
        let bounds = Bounds::new(point(px(100.0), px(0.0)), size(px(200.0), px(24.0)));
        assert!((fraction_within(bounds, point(px(150.0), px(5.0))) - 0.25).abs() < f64::EPSILON);
        assert!(fraction_within(bounds, point(px(40.0), px(5.0))).abs() < f64::EPSILON);
        assert!((fraction_within(bounds, point(px(900.0), px(5.0))) - 1.0).abs() < f64::EPSILON);
    }
}
