use gpui_kit::base::{set_reduce_motion_preference, ReduceMotionPreference};
use gpui_kit::App;

pub(crate) fn follow_motion_preference(setting_reduces: bool, cx: &mut App) {
    set_reduce_motion_preference(preference_for(setting_reduces), cx);
}

const fn preference_for(setting_reduces: bool) -> ReduceMotionPreference {
    if setting_reduces {
        ReduceMotionPreference::Reduce
    } else {
        ReduceMotionPreference::System
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_forces_reduced_motion_and_otherwise_the_system_decides() {
        assert_eq!(preference_for(true), ReduceMotionPreference::Reduce);
        assert_eq!(preference_for(false), ReduceMotionPreference::System);
    }
}
