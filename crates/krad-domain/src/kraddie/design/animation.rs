use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const ANIMATION_FRAME: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Pose {
    pub dx: i8,
    pub dy: i8,
    pub squash: i8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KraddieAnimation {
    #[default]
    Still,
    Breathe,
    Bounce,
    Sway,
    Shake,
    Hop,
}

const BREATHE: [i8; 8] = [0, 0, 0, 1, 1, 1, 0, 0];
const BOUNCE: [i8; 6] = [0, -1, -2, -2, -1, 0];
const SWAY: [i8; 8] = [0, 1, 1, 0, 0, -1, -1, 0];
const SHAKE: [i8; 4] = [1, -1, 1, -1];
const HOP: [i8; 10] = [0, 0, 0, 0, -1, -3, -4, -3, -1, 0];

impl KraddieAnimation {
    pub const ALL: [Self; 6] = [
        Self::Still,
        Self::Breathe,
        Self::Bounce,
        Self::Sway,
        Self::Shake,
        Self::Hop,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Still => "Still",
            Self::Breathe => "Breathe",
            Self::Bounce => "Bounce",
            Self::Sway => "Sway",
            Self::Shake => "Shake",
            Self::Hop => "Hop",
        }
    }

    #[must_use]
    pub const fn moves(self) -> bool {
        !matches!(self, Self::Still)
    }

    #[must_use]
    pub fn pose(self, frame: u64) -> Pose {
        let pick = |steps: &[i8]| {
            let len = u64::try_from(steps.len()).unwrap_or(1);
            usize::try_from(frame % len)
                .ok()
                .and_then(|step| steps.get(step).copied())
                .unwrap_or_default()
        };
        match self {
            Self::Still => Pose::default(),
            Self::Breathe => Pose {
                squash: pick(&BREATHE),
                ..Pose::default()
            },
            Self::Bounce => Pose {
                dy: pick(&BOUNCE),
                ..Pose::default()
            },
            Self::Sway => Pose {
                dx: pick(&SWAY),
                ..Pose::default()
            },
            Self::Shake => Pose {
                dx: pick(&SHAKE),
                ..Pose::default()
            },
            Self::Hop => Pose {
                dy: pick(&HOP),
                ..Pose::default()
            },
        }
    }
}

#[must_use]
pub fn animation_frame(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_millis() / ANIMATION_FRAME.as_millis()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_kraddie_never_moves_and_the_others_loop() {
        for frame in 0..20 {
            assert_eq!(KraddieAnimation::Still.pose(frame), Pose::default());
        }
        let hop = |frame| KraddieAnimation::Hop.pose(frame).dy;
        assert_eq!(hop(6), -4);
        assert_eq!(hop(6 + 10), -4);
        assert_eq!(KraddieAnimation::Shake.pose(1).dx, -1);
        assert_eq!(
            animation_frame(ANIMATION_FRAME * 7 + Duration::from_millis(1)),
            7
        );
        let moving = KraddieAnimation::ALL
            .iter()
            .filter(|animation| animation.moves())
            .count();
        assert_eq!(moving, 5);
    }
}
