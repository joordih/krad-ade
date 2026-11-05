use gpui_kit::{
    canvas, AnyElement, Context, DispatchPhase, HitboxBehavior, IntoElement, ScrollDelta,
    ScrollWheelEvent, Styled,
};
use std::time::Instant;

use super::{BandKey, BandWheel, Pointer, ScrollEdges, Wheel};
use crate::environment_shell::EnvironmentShell;

pub(in crate::environment_shell) fn band_watch(
    key: BandKey,
    edges: impl Fn() -> ScrollEdges + 'static,
    cx: &mut Context<EnvironmentShell>,
) -> AnyElement {
    let shell = cx.entity().downgrade();
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let ScrollDelta::Pixels(delta) = event.delta else {
                    return;
                };
                let pointer = if hitbox.should_handle_scroll(window) {
                    Pointer::Over
                } else {
                    Pointer::Away
                };
                let wheel = Wheel {
                    dx: delta.x.as_f32(),
                    dy: delta.y.as_f32(),
                    phase: event.touch_phase,
                    pointer,
                };
                let outcome = shell
                    .update(cx, |shell, cx| {
                        let outcome =
                            shell
                                .gestures
                                .bands
                                .wheel(key, wheel, edges(), Instant::now());
                        if outcome != BandWheel::Pass {
                            cx.notify();
                        }
                        outcome
                    })
                    .unwrap_or(BandWheel::Pass);
                if outcome == BandWheel::Taken {
                    cx.stop_propagation();
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
