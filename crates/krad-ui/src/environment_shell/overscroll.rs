use gpui_kit::base::{ScrollBounce, ScrollBounceMotion, ScrollbarHandle};
use gpui_kit::{ElementId, IntoElement};
use std::time::Duration;

const TRACKING: f32 = 0.55;
const MAX_STRETCH: f32 = 120.0;
const RESPONSE: Duration = Duration::from_millis(350);

fn motion() -> ScrollBounceMotion {
    ScrollBounceMotion::default()
        .with_tracking(TRACKING)
        .with_response(RESPONSE)
        .with_max_stretch(MAX_STRETCH)
}

pub(super) fn overscroll<H: ScrollbarHandle + Clone>(
    id: impl Into<ElementId>,
    handle: &H,
    child: impl IntoElement,
) -> ScrollBounce {
    ScrollBounce::new(id, handle, child).motion(motion())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_edge_stretches_and_settles_like_the_hand_made_band_did() {
        let motion = motion();
        assert!((motion.tracking() - TRACKING).abs() < f32::EPSILON);
        assert_eq!(motion.response(), RESPONSE);
        assert_eq!(motion.max_stretch(), Some(MAX_STRETCH));
    }
}
