use gpui_kit::{ScrollDelta, ScrollWheelEvent, TouchPhase};
use krad_domain::TerminalId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::motion::Spring;

const RESISTANCE: f32 = 0.55;
const REACH: f32 = 120.0;
const EDGE_SLACK: f32 = 0.5;
const RESPONSE_S: f32 = 0.35;
const MAX_FRAME_S: f32 = 1.0 / 30.0;
const IDLE: Duration = Duration::from_millis(90);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum BandKey {
    Chat(TerminalId),
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ScrollEdges {
    pub(super) offset: f32,
    pub(super) max: f32,
}

impl ScrollEdges {
    fn at_top(self) -> bool {
        self.offset <= EDGE_SLACK
    }

    fn at_bottom(self) -> bool {
        self.offset >= self.max - EDGE_SLACK
    }
}

#[derive(Clone, Copy, Debug)]
struct Band {
    pull: f32,
    last: Instant,
    release: Option<(Spring, Instant)>,
}

fn stretched(pull: f32) -> f32 {
    let distance = pull.abs();
    let stretch = (1.0 - 1.0 / (distance * RESISTANCE / REACH + 1.0)) * REACH;
    stretch.copysign(pull)
}

#[derive(Default)]
pub(super) struct RubberBands {
    bands: HashMap<BandKey, Band>,
}

impl RubberBands {
    pub(super) fn offset(&self, key: BandKey) -> f32 {
        self.bands.get(&key).map_or(0.0, |band| match band.release {
            Some((spring, _)) => spring.value(),
            None => stretched(band.pull),
        })
    }

    pub(super) fn wheel(
        &mut self,
        key: BandKey,
        event: &ScrollWheelEvent,
        edges: ScrollEdges,
    ) -> bool {
        let ScrollDelta::Pixels(delta) = event.delta else {
            return false;
        };
        let dy = delta.y.as_f32();
        let now = Instant::now();
        if event.touch_phase == TouchPhase::Ended {
            return self.release(key, now);
        }
        let band = self.bands.get(&key).copied();
        let pulling = band.is_some_and(|band| band.release.is_none() && band.pull != 0.0);
        let past_top = dy > 0.0 && edges.at_top();
        let past_bottom = dy < 0.0 && edges.at_bottom();
        if !pulling && !past_top && !past_bottom {
            return false;
        }
        let pull = band
            .filter(|band| band.release.is_none())
            .map_or(0.0, |band| band.pull)
            + dy;
        let pull = if pulling && pull.signum() != band.map_or(0.0, |band| band.pull).signum() {
            0.0
        } else {
            pull
        };
        self.bands.insert(
            key,
            Band {
                pull,
                last: now,
                release: None,
            },
        );
        true
    }

    fn release(&mut self, key: BandKey, now: Instant) -> bool {
        let Some(band) = self.bands.get_mut(&key) else {
            return false;
        };
        if band.release.is_some() {
            return false;
        }
        let mut spring = Spring::at_rest(stretched(band.pull), RESPONSE_S);
        spring.set_target(0.0);
        band.release = Some((spring, now));
        true
    }

    pub(super) fn advance(&mut self, now: Instant) -> bool {
        let idle: Vec<BandKey> = self
            .bands
            .iter()
            .filter(|(_, band)| band.release.is_none() && now.duration_since(band.last) > IDLE)
            .map(|(key, _)| *key)
            .collect();
        for key in idle {
            self.release(key, now);
        }
        let mut moving = false;
        self.bands.retain(|_, band| {
            let Some((mut spring, last)) = band.release else {
                moving = true;
                return true;
            };
            spring.step(now.duration_since(last).as_secs_f32().min(MAX_FRAME_S));
            band.release = Some((spring, now));
            let settled = spring.is_settled();
            moving |= !settled;
            !settled
        });
        moving
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stretch_resists_more_the_further_it_is_pulled_and_keeps_its_direction() {
        let small = stretched(20.0);
        let large = stretched(400.0);
        assert!(small > 0.0 && small < 20.0);
        assert!(large < REACH);
        assert!(large - stretched(300.0) < stretched(100.0) - small);
        assert!(stretched(-50.0) < 0.0);
    }
}
