use serde::{Deserialize, Serialize};
use std::time::Duration;

const SHORT: Duration = Duration::from_secs(3);
const NORMAL: Duration = Duration::from_secs(6);
const LONG: Duration = Duration::from_secs(12);
const HALF: f32 = 0.5;
const FRACTION_STEPS: u16 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToastKind {
    Error,
    Warning,
    Success,
    Info,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToastLifetime {
    Short,
    #[default]
    Normal,
    Long,
    UntilClosed,
}

impl ToastLifetime {
    pub const ALL: [Self; 4] = [Self::Short, Self::Normal, Self::Long, Self::UntilClosed];

    #[must_use]
    pub const fn duration(self) -> Option<Duration> {
        match self {
            Self::Short => Some(SHORT),
            Self::Normal => Some(NORMAL),
            Self::Long => Some(LONG),
            Self::UntilClosed => None,
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Short => "3 s",
            Self::Normal => "6 s",
            Self::Long => "12 s",
            Self::UntilClosed => "Until closed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "f32", into = "f32")]
pub struct WindowFraction(u16);

impl WindowFraction {
    #[must_use]
    pub fn new(value: f32) -> Self {
        let clamped = if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            HALF
        };
        Self(round_steps(clamped * f32::from(FRACTION_STEPS)))
    }

    #[must_use]
    pub fn value(self) -> f32 {
        f32::from(self.0) / f32::from(FRACTION_STEPS)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn round_steps(steps: f32) -> u16 {
    steps.round() as u16
}

impl From<f32> for WindowFraction {
    fn from(value: f32) -> Self {
        Self::new(value)
    }
}

impl From<WindowFraction> for f32 {
    fn from(fraction: WindowFraction) -> Self {
        fraction.value()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackDirection {
    Down,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToastPlacement {
    pub x: WindowFraction,
    pub y: WindowFraction,
}

impl ToastPlacement {
    #[must_use]
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: WindowFraction::new(x),
            y: WindowFraction::new(y),
        }
    }

    #[must_use]
    pub fn side(self) -> WindowSide {
        if self.x.value() < HALF {
            WindowSide::Left
        } else {
            WindowSide::Right
        }
    }

    #[must_use]
    pub fn direction(self) -> StackDirection {
        if self.y.value() < HALF {
            StackDirection::Down
        } else {
            StackDirection::Up
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToastSettings {
    #[serde(default)]
    pub lifetime: ToastLifetime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<ToastPlacement>,
}

impl ToastSettings {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_left_half_places_toasts_on_the_left_and_the_lower_half_stacks_them_up() {
        let top_left = ToastPlacement::new(0.1, 0.1);
        assert_eq!(top_left.side(), WindowSide::Left);
        assert_eq!(top_left.direction(), StackDirection::Down);
        let bottom_right = ToastPlacement::new(0.5, 0.9);
        assert_eq!(bottom_right.side(), WindowSide::Right);
        assert_eq!(bottom_right.direction(), StackDirection::Up);
    }

    #[test]
    fn a_placement_stays_inside_the_window() {
        let placement = ToastPlacement::new(-3.0, f32::NAN);
        assert!(placement.x.value().abs() < f32::EPSILON);
        assert!((placement.y.value() - HALF).abs() < f32::EPSILON);
    }

    #[test]
    fn settings_round_trip_and_clamp_a_stored_placement() -> serde_json::Result<()> {
        let settings = ToastSettings {
            lifetime: ToastLifetime::UntilClosed,
            placement: Some(ToastPlacement::new(0.25, 0.75)),
        };
        let text = serde_json::to_string(&settings)?;
        assert_eq!(
            text,
            r#"{"lifetime":"until_closed","placement":{"x":0.25,"y":0.75}}"#
        );
        assert_eq!(serde_json::from_str::<ToastSettings>(&text)?, settings);
        let stored: ToastSettings = serde_json::from_str(r#"{"placement":{"x":4,"y":0.5}}"#)?;
        assert_eq!(stored.placement, Some(ToastPlacement::new(1.0, 0.5)));
        assert_eq!(stored.lifetime, ToastLifetime::Normal);
        Ok(())
    }

    #[test]
    fn only_until_closed_has_no_duration() {
        assert!(ToastLifetime::ALL
            .iter()
            .all(|lifetime| lifetime.duration().is_none()
                == (*lifetime == ToastLifetime::UntilClosed)));
    }
}
