mod watch;

use gpui_kit::TouchPhase;
use krad_domain::TerminalId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::motion::Spring;

pub(super) use watch::band_watch;

const RESISTANCE: f32 = 0.55;
const REACH: f32 = 120.0;
const MAX_STRETCH: f32 = REACH * 0.99;
const EDGE_SLACK: f32 = 0.5;
const RESPONSE_S: f32 = 0.35;
const MAX_FRAME_S: f32 = 1.0 / 30.0;
const COAST_GAP: Duration = Duration::from_millis(90);
const MOMENTUM_GAIN: f32 = 60.0;
const MAX_BOUNCE_SPEED: f32 = 1200.0;

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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Overflow {
    Inside,
    AtEdge(f32),
    Crossing(f32),
}

impl Overflow {
    fn past_edge(self) -> Option<f32> {
        match self {
            Self::Inside => None,
            Self::AtEdge(excess) | Self::Crossing(excess) => Some(excess),
        }
    }
}

impl ScrollEdges {
    fn overflow(self, dy: f32) -> Overflow {
        let (room, excess) = if dy > 0.0 {
            let room = self.offset.max(0.0);
            (room, (dy - room).max(0.0))
        } else {
            let room = (self.max - self.offset).max(0.0);
            (room, (dy + room).min(0.0))
        };
        if dy == 0.0 {
            Overflow::Inside
        } else if room <= EDGE_SLACK {
            Overflow::AtEdge(dy)
        } else if excess == 0.0 {
            Overflow::Inside
        } else {
            Overflow::Crossing(excess)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pointer {
    Over,
    Away,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Wheel {
    pub(super) dx: f32,
    pub(super) dy: f32,
    pub(super) phase: TouchPhase,
    pub(super) pointer: Pointer,
}

impl Wheel {
    fn is_vertical(self) -> bool {
        self.dy.abs() > self.dx.abs()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BandWheel {
    Pass,
    Shared,
    Taken,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Finger {
    #[default]
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Coast {
    #[default]
    Fresh,
    Spent,
}

#[derive(Clone, Copy, Debug)]
enum Band {
    Held { pull: f32 },
    Settling { spring: Spring, last: Instant },
}

impl Band {
    fn offset(self) -> f32 {
        match self {
            Self::Held { pull } => stretched(pull),
            Self::Settling { spring, .. } => spring.value(),
        }
    }

    fn released(self, now: Instant) -> Self {
        let mut spring = Spring::at_rest(self.offset(), RESPONSE_S);
        spring.set_target(0.0);
        Self::Settling { spring, last: now }
    }
}

fn stretched(pull: f32) -> f32 {
    let distance = pull.abs();
    let stretch = (1.0 - 1.0 / (distance * RESISTANCE / REACH + 1.0)) * REACH;
    stretch.copysign(pull)
}

fn unstretched(stretch: f32) -> f32 {
    let shown = stretch.abs().min(MAX_STRETCH);
    (shown * REACH / ((REACH - shown) * RESISTANCE)).copysign(stretch)
}

#[derive(Default)]
pub(super) struct RubberBands {
    bands: HashMap<BandKey, Band>,
    finger: Finger,
    coast: Coast,
    last_wheel: Option<Instant>,
}

impl RubberBands {
    pub(super) fn offset(&self, key: BandKey) -> f32 {
        self.bands.get(&key).map_or(0.0, |band| band.offset())
    }

    pub(super) fn wheel(
        &mut self,
        key: BandKey,
        wheel: Wheel,
        edges: ScrollEdges,
        now: Instant,
    ) -> BandWheel {
        let quiet = self
            .last_wheel
            .is_none_or(|last| now.duration_since(last) > COAST_GAP);
        self.last_wheel = Some(now);
        match wheel.phase {
            TouchPhase::Ended | TouchPhase::Cancelled => self.lift(now),
            TouchPhase::Started => {
                self.finger = Finger::Down;
                self.coast = Coast::Fresh;
                let caught = wheel.pointer == Pointer::Over && self.catch(key);
                match self.drag(key, wheel, edges) {
                    BandWheel::Pass if caught => BandWheel::Taken,
                    outcome => outcome,
                }
            }
            TouchPhase::Moved if self.finger == Finger::Down => self.drag(key, wheel, edges),
            TouchPhase::Moved => {
                if quiet {
                    self.coast = Coast::Fresh;
                }
                self.glide(key, wheel, edges, now)
            }
        }
    }

    fn lift(&mut self, now: Instant) -> BandWheel {
        self.finger = Finger::Up;
        let mut released = false;
        for band in self.bands.values_mut() {
            if let Band::Held { .. } = band {
                *band = band.released(now);
                released = true;
            }
        }
        self.coast = if released { Coast::Spent } else { Coast::Fresh };
        if released {
            BandWheel::Shared
        } else {
            BandWheel::Pass
        }
    }

    fn catch(&mut self, key: BandKey) -> bool {
        let Some(band) = self.bands.get_mut(&key) else {
            return false;
        };
        let Band::Settling { spring, .. } = *band else {
            return false;
        };
        *band = Band::Held {
            pull: unstretched(spring.value()),
        };
        true
    }

    fn drag(&mut self, key: BandKey, wheel: Wheel, edges: ScrollEdges) -> BandWheel {
        if let Some(Band::Held { pull }) = self.bands.get(&key).copied() {
            let next = pull + wheel.dy;
            if next == 0.0 || next.signum() != pull.signum() {
                self.bands.remove(&key);
            } else {
                self.bands.insert(key, Band::Held { pull: next });
            }
            return BandWheel::Taken;
        }
        if wheel.pointer == Pointer::Away || !wheel.is_vertical() {
            return BandWheel::Pass;
        }
        let (pull, outcome) = match edges.overflow(wheel.dy) {
            Overflow::Inside => return BandWheel::Pass,
            Overflow::AtEdge(pull) => (pull, BandWheel::Taken),
            Overflow::Crossing(pull) => (pull, BandWheel::Shared),
        };
        self.bands.insert(key, Band::Held { pull });
        outcome
    }

    fn glide(&mut self, key: BandKey, wheel: Wheel, edges: ScrollEdges, now: Instant) -> BandWheel {
        if let Some(band @ Band::Held { .. }) = self.bands.get(&key).copied() {
            self.bands.insert(key, band.released(now));
            self.coast = Coast::Spent;
        }
        if wheel.pointer == Pointer::Away || !wheel.is_vertical() {
            return BandWheel::Pass;
        }
        let Some(overflow) = edges.overflow(wheel.dy).past_edge() else {
            return BandWheel::Pass;
        };
        if self.bands.contains_key(&key) {
            return BandWheel::Taken;
        }
        if self.coast == Coast::Spent {
            return BandWheel::Pass;
        }
        self.coast = Coast::Spent;
        let mut spring = Spring::at_rest(0.0, RESPONSE_S);
        spring.kick((overflow * MOMENTUM_GAIN).clamp(-MAX_BOUNCE_SPEED, MAX_BOUNCE_SPEED));
        self.bands.insert(key, Band::Settling { spring, last: now });
        BandWheel::Shared
    }

    pub(super) fn advance(&mut self, now: Instant) -> bool {
        let mut moving = false;
        self.bands.retain(|_, band| {
            let Band::Settling { spring, last } = band else {
                return true;
            };
            spring.step(now.duration_since(*last).as_secs_f32().min(MAX_FRAME_S));
            *last = now;
            let settled = spring.is_settled();
            moving |= !settled;
            !settled
        });
        moving
    }
}

#[cfg(test)]
mod tests;
