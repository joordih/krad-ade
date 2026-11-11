use gpui_kit::{App, Global};

#[cfg(target_os = "linux")]
use super::linux as platform;
#[cfg(target_os = "macos")]
use super::macos as platform;
#[cfg(windows)]
use super::windows as platform;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    pub(super) const fn reduces_motion() -> bool {
        false
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct MotionPreference {
    setting: bool,
    system: bool,
}

impl MotionPreference {
    const fn reduces(self) -> bool {
        self.setting || self.system
    }
}

impl Global for MotionPreference {}

pub(crate) fn follow_motion_preference(setting_reduces: bool, cx: &mut App) {
    let preference = MotionPreference {
        setting: setting_reduces,
        ..current(cx)
    };
    store(preference, cx);
    let read = cx
        .background_executor()
        .spawn(async { platform::reduces_motion() });
    cx.spawn(async move |cx| {
        let system = read.await;
        cx.update(|cx| {
            let preference = MotionPreference {
                system,
                ..current(cx)
            };
            store(preference, cx);
        });
    })
    .detach();
}

fn current(cx: &App) -> MotionPreference {
    cx.try_global::<MotionPreference>()
        .copied()
        .unwrap_or_default()
}

fn store(preference: MotionPreference, cx: &mut App) {
    cx.set_global(preference);
    cx.set_reduce_motion(preference.reduces());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn either_the_setting_or_the_system_reduces_motion() {
        for (setting, system, expected) in [
            (false, false, false),
            (true, false, true),
            (false, true, true),
            (true, true, true),
        ] {
            assert_eq!(
                MotionPreference { setting, system }.reduces(),
                expected,
                "setting {setting}, system {system}"
            );
        }
    }
}
