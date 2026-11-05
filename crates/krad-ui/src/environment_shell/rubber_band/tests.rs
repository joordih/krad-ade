use super::*;

const FRAME: Duration = Duration::from_millis(16);
const KEY: BandKey = BandKey::Settings;
const STEP: f32 = 8.0;
const PULL_STEPS: usize = 10;
const MAX_FRAMES: usize = 600;
const SAME: f32 = 0.01;
const TOP: ScrollEdges = ScrollEdges {
    offset: 0.0,
    max: 500.0,
};

struct Rig {
    bands: RubberBands,
    now: Instant,
}

impl Rig {
    fn new() -> Self {
        Self {
            bands: RubberBands::default(),
            now: Instant::now(),
        }
    }

    fn send(&mut self, phase: TouchPhase, dy: f32, edges: ScrollEdges) -> BandWheel {
        self.aim(phase, (0.0, dy), Pointer::Over, edges)
    }

    fn aim(
        &mut self,
        phase: TouchPhase,
        (dx, dy): (f32, f32),
        pointer: Pointer,
        edges: ScrollEdges,
    ) -> BandWheel {
        self.now += FRAME;
        let wheel = Wheel {
            dx,
            dy,
            phase,
            pointer,
        };
        self.bands.wheel(KEY, wheel, edges, self.now)
    }

    fn tick(&mut self) -> bool {
        self.now += FRAME;
        self.bands.advance(self.now)
    }

    fn offset(&self) -> f32 {
        self.bands.offset(KEY)
    }

    fn pull(&mut self) {
        self.send(TouchPhase::Started, 0.0, TOP);
        for _ in 0..PULL_STEPS {
            assert_eq!(self.send(TouchPhase::Moved, STEP, TOP), BandWheel::Taken);
        }
    }

    fn settle(&mut self) -> Vec<f32> {
        let mut path = Vec::new();
        for _ in 0..MAX_FRAMES {
            let moving = self.tick();
            path.push(self.offset());
            if !moving {
                break;
            }
        }
        path
    }
}

#[test]
fn the_stretch_resists_more_the_further_it_is_pulled_and_keeps_its_direction() {
    let small = stretched(20.0);
    let large = stretched(400.0);
    assert!(small > 0.0 && small < 20.0);
    assert!(large < REACH);
    assert!(large - stretched(300.0) < stretched(100.0) - small);
    assert!(stretched(-50.0) < 0.0);
    assert!((stretched(unstretched(-37.0)) + 37.0).abs() < SAME);
}

#[test]
fn pulling_past_the_top_takes_the_wheel_with_growing_resistance() {
    let mut rig = Rig::new();
    rig.send(TouchPhase::Started, 0.0, TOP);
    let mut gains = Vec::new();
    for _ in 0..PULL_STEPS {
        let before = rig.offset();
        assert_eq!(rig.send(TouchPhase::Moved, STEP, TOP), BandWheel::Taken);
        gains.push(rig.offset() - before);
    }
    assert!(gains
        .windows(2)
        .all(|pair| pair[1] < pair[0] && pair[1] > 0.0));
}

#[test]
fn scrolling_into_the_edge_shares_the_wheel_and_stretches_only_the_excess() {
    let mut rig = Rig::new();
    rig.send(TouchPhase::Started, 0.0, TOP);
    let near = ScrollEdges { offset: 3.0, ..TOP };
    assert_eq!(rig.send(TouchPhase::Moved, 10.0, near), BandWheel::Shared);
    assert!((rig.offset() - stretched(7.0)).abs() < SAME);
}

#[test]
fn a_mostly_horizontal_swipe_at_the_edge_is_left_alone() {
    let mut rig = Rig::new();
    let wheel = rig.aim(TouchPhase::Moved, (30.0, 2.0), Pointer::Over, TOP);
    assert_eq!(wheel, BandWheel::Pass);
    assert!(rig.offset().abs() < f32::EPSILON);
}

#[test]
fn reversing_shrinks_the_stretch_smoothly_then_hands_back_scrolling() {
    let mut rig = Rig::new();
    rig.pull();
    let mut previous = rig.offset();
    let mut handed_back = false;
    for _ in 0..MAX_FRAMES {
        let wheel = rig.send(TouchPhase::Moved, -STEP, TOP);
        let offset = rig.offset();
        if wheel == BandWheel::Pass {
            assert!(offset.abs() < f32::EPSILON);
            handed_back = true;
            break;
        }
        assert_eq!(wheel, BandWheel::Taken);
        assert!(offset <= previous && offset >= 0.0);
        assert!(previous - offset <= STEP);
        previous = offset;
    }
    assert!(handed_back, "scrolling returns to the content");
}

#[test]
fn release_returns_to_rest_without_overshoot() {
    let mut rig = Rig::new();
    rig.pull();
    let start = rig.offset();
    assert_eq!(rig.send(TouchPhase::Ended, 0.0, TOP), BandWheel::Shared);
    let path = rig.settle();
    assert!(path.iter().all(|offset| *offset >= 0.0 && *offset <= start));
    assert!(path.windows(2).all(|pair| pair[1] <= pair[0]));
    assert!(rig.offset().abs() < f32::EPSILON);
    assert!(!rig.tick());
}

#[test]
fn the_end_of_a_touch_releases_a_band_the_pointer_left() {
    let mut rig = Rig::new();
    rig.pull();
    rig.aim(TouchPhase::Ended, (0.0, 0.0), Pointer::Away, TOP);
    rig.settle();
    assert!(rig.offset().abs() < f32::EPSILON);
}

#[test]
fn a_touch_during_release_continues_from_the_animated_offset() {
    let mut rig = Rig::new();
    rig.pull();
    rig.send(TouchPhase::Ended, 0.0, TOP);
    (0..PULL_STEPS / 2).for_each(|_| {
        rig.tick();
    });
    let caught = rig.offset();
    assert!(caught > 0.0);
    assert_eq!(rig.send(TouchPhase::Started, 0.0, TOP), BandWheel::Taken);
    assert!((rig.offset() - caught).abs() < SAME);
    rig.tick();
    assert!((rig.offset() - caught).abs() < SAME, "the band is held");
    rig.send(TouchPhase::Moved, STEP, TOP);
    let pulled = rig.offset() - caught;
    assert!(pulled > 0.0 && pulled < STEP);
}

#[test]
fn momentum_after_release_does_not_restart_the_pull() {
    let mut rig = Rig::new();
    rig.pull();
    rig.send(TouchPhase::Ended, 0.0, TOP);
    let mut previous = rig.offset();
    for _ in 0..MAX_FRAMES {
        let before = rig.offset();
        let wheel = rig.send(TouchPhase::Moved, STEP, TOP);
        assert!((rig.offset() - before).abs() < f32::EPSILON);
        if !rig.tick() {
            assert_eq!(wheel, BandWheel::Taken);
            break;
        }
        assert!(rig.offset() <= previous);
        previous = rig.offset();
    }
    assert!(rig.offset().abs() < f32::EPSILON);
    assert_eq!(rig.send(TouchPhase::Moved, STEP, TOP), BandWheel::Pass);
}

#[test]
fn momentum_hitting_an_edge_bounces_once_and_settles() {
    let mut rig = Rig::new();
    assert_eq!(rig.send(TouchPhase::Moved, STEP, TOP), BandWheel::Shared);
    let mut path = Vec::new();
    for _ in 0..MAX_FRAMES {
        rig.send(TouchPhase::Moved, STEP, TOP);
        let moving = rig.tick();
        path.push(rig.offset());
        if !moving {
            break;
        }
    }
    let peak = path
        .iter()
        .position(|offset| path.iter().all(|other| other <= offset))
        .unwrap_or_default();
    assert!(path[peak] > 0.0 && path[peak] < REACH / 4.0);
    assert!(path[..=peak].windows(2).all(|pair| pair[1] >= pair[0]));
    assert!(path[peak..].windows(2).all(|pair| pair[1] <= pair[0]));
    assert!(path.iter().all(|offset| *offset >= 0.0));
    assert!(rig.offset().abs() < f32::EPSILON);
    assert_eq!(rig.send(TouchPhase::Moved, STEP, TOP), BandWheel::Pass);
}
