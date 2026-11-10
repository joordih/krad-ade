use super::*;

const WIDTHS: [f32; 5] = [640.0, 900.0, 1280.0, 1920.0, 2560.0];
const CODEX: [Effort; 6] = [
    Effort::Low,
    Effort::Medium,
    Effort::High,
    Effort::ExtraHigh,
    Effort::Max,
    Effort::Ultra,
];

fn close(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.001
}

#[test]
fn the_thumb_spans_the_track_from_the_first_to_the_last_allowed_level() {
    assert!(close(thumb_left(0, CODEX.len()), 0.0));
    assert!(close(thumb_left(5, CODEX.len()), TRACK_W - THUMB_W));
    assert!(close(fill_width(5, CODEX.len()), TRACK_W));
    assert!(close(thumb_left(1, 3), (TRACK_W - THUMB_W) / 2.0));
    let lefts: Vec<f32> = (0..CODEX.len())
        .map(|step| thumb_left(step, CODEX.len()))
        .collect();
    assert!(lefts.windows(2).all(|pair| pair[0] < pair[1]), "{lefts:?}");
}

#[test]
fn levels_map_to_a_share_of_the_scale() {
    assert!(close(level_share(0, 0), 0.0));
    assert!(close(level_share(0, 1), 1.0));
    assert!(close(level_share(2, 5), 0.5));
    assert!(close(level_share(9, 5), 1.0));
}

#[test]
fn one_or_no_level_disables_the_trigger_and_no_effort_hides_it() {
    assert_eq!(trigger(false, 5), Trigger::Hidden);
    assert_eq!(trigger(true, 0), Trigger::Disabled);
    assert_eq!(trigger(true, 1), Trigger::Disabled);
    assert_eq!(trigger(true, 2), Trigger::Enabled);
}

#[test]
fn arrows_step_and_home_and_end_jump_within_the_allowed_levels() {
    let from = Some(Effort::High);
    assert_eq!(
        stepped(&CODEX, from, SlideKey::Higher),
        Some(Effort::ExtraHigh)
    );
    assert_eq!(stepped(&CODEX, from, SlideKey::Lower), Some(Effort::Medium));
    assert_eq!(stepped(&CODEX, from, SlideKey::Lowest), Some(Effort::Low));
    assert_eq!(
        stepped(&CODEX, from, SlideKey::Highest),
        Some(Effort::Ultra)
    );
    assert_eq!(
        stepped(&CODEX, Some(Effort::Ultra), SlideKey::Higher),
        Some(Effort::Ultra)
    );
    assert_eq!(stepped(&CODEX, None, SlideKey::Higher), Some(Effort::Low));
    assert_eq!(stepped(&[], from, SlideKey::Higher), None);
}

#[test]
fn the_texture_gets_denser_and_brighter_with_more_effort() {
    let columns = dot_columns();
    assert!(columns > 0);
    let lit = |strength: f32| {
        (0..columns)
            .flat_map(|column| (0..DOT_ROWS).map(move |row| (column, row)))
            .filter_map(|(column, row)| dot_alpha(column, row, columns, strength))
            .collect::<Vec<u32>>()
    };
    let (low, high) = (lit(0.2), lit(1.0));
    assert!(high.len() > low.len(), "{} vs {}", high.len(), low.len());
    let brightest = |dots: &[u32]| dots.iter().copied().max().unwrap_or_default();
    assert!(brightest(&high) > brightest(&low));
    assert_eq!(lit(0.7), lit(0.7), "the pattern is the same on every frame");
}

#[test]
fn the_level_tone_moves_toward_the_accent() {
    assert!(close(level_tone(0.0), MIN_TONE));
    assert!(close(level_tone(1.0), 1.0));
    let from = Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    let to = Rgba {
        r: 1.0,
        g: 0.5,
        b: 0.0,
        a: 1.0,
    };
    let half = mix(from, to, 0.5);
    assert!(close(half.r, 0.5) && close(half.g, 0.25) && close(half.a, 1.0));
}

#[test]
fn the_snap_overshoots_a_little_then_settles() {
    assert!(close(snap_bounce(0.0), 0.0));
    assert!(close(snap_bounce(1.0), 1.0));
    let peak = (1..100_u8)
        .map(|step| snap_bounce(f32::from(step) / 100.0))
        .fold(0.0_f32, f32::max);
    assert!(peak > 1.0 && peak < 1.15, "{peak}");
}

#[test]
fn the_popover_stays_inside_the_window_at_every_width() {
    for width in WIDTHS {
        for anchor in [0.0, width / 2.0, width] {
            let left = popover_left(anchor, width);
            assert!(left >= EDGE_MARGIN, "{width} {anchor}: {left}");
            assert!(
                left + POPOVER_W <= width - EDGE_MARGIN + 0.001,
                "{width} {anchor}: {left}"
            );
        }
    }
    assert!(close(popover_left(640.0, 1280.0), 640.0 - POPOVER_W / 2.0));
}

#[test]
fn the_popover_keys() {
    assert_eq!(popover_key("escape"), Some(PopoverKey::Close));
    assert_eq!(popover_key("?"), Some(PopoverKey::Help));
    assert_eq!(
        popover_key("home"),
        Some(PopoverKey::Slide(SlideKey::Lowest))
    );
    assert_eq!(popover_key("a"), None);
}
