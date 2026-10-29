use gpui_kit::{radians, Svg, Transformation};
use krad_domain::AgentState;
use std::f32::consts::FRAC_PI_4;
use std::sync::OnceLock;
use std::time::Instant;

use super::icon::{icon, IconName};
use crate::theme::Palette;

const SPIN_PERIOD_MS: u128 = 1_200;
const SPIN_SYMMETRY: f32 = FRAC_PI_4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StatusKind {
    Agent(AgentState),
    Failed,
}

impl StatusKind {
    pub(crate) const fn look(self, pal: &Palette) -> (IconName, u32) {
        match self {
            Self::Agent(AgentState::Idle) => (IconName::Tick, pal.text_muted),
            Self::Agent(AgentState::Working) => (IconName::Loading, pal.text_muted),
            Self::Agent(AgentState::WaitingForInput) => (IconName::RadioButton, pal.attention),
            Self::Agent(AgentState::Done) => (IconName::CheckCircle, pal.text_strong),
            Self::Failed => (IconName::CancelCircle, pal.error_fg),
        }
    }
}

pub(crate) fn agent_status(kind: StatusKind, size: f32, pal: &Palette) -> Svg {
    let (name, color) = kind.look(pal);
    let glyph = icon(name, size, color);
    if kind == StatusKind::Agent(AgentState::Working) {
        glyph.with_transformation(Transformation::rotate(radians(spin_angle(Instant::now()))))
    } else {
        glyph
    }
}

fn spin_angle(now: Instant) -> f32 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = *START.get_or_init(|| now);
    let phase = now.saturating_duration_since(start).as_millis() % SPIN_PERIOD_MS;
    let fraction = u16::try_from(phase).map_or(0.0, f32::from)
        / u16::try_from(SPIN_PERIOD_MS).map_or(1.0, f32::from);
    fraction * SPIN_SYMMETRY
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Appearance;
    use std::time::Duration;

    #[test]
    fn only_waiting_is_amber_and_failure_is_red() {
        let pal = Appearance::Light.palette();
        assert_eq!(
            StatusKind::Agent(AgentState::WaitingForInput).look(pal),
            (IconName::RadioButton, pal.attention)
        );
        assert_eq!(
            StatusKind::Failed.look(pal),
            (IconName::CancelCircle, pal.error_fg)
        );
        for state in [AgentState::Idle, AgentState::Working, AgentState::Done] {
            assert_ne!(StatusKind::Agent(state).look(pal).1, pal.attention);
        }
    }

    #[test]
    fn spinner_stays_within_one_symmetric_step() {
        let start = Instant::now();
        let first = spin_angle(start);
        for step in 0..40 {
            let angle = spin_angle(start + Duration::from_millis(step * 97));
            assert!((0.0..SPIN_SYMMETRY).contains(&angle), "{angle}");
        }
        assert!(first < SPIN_SYMMETRY);
    }
}
