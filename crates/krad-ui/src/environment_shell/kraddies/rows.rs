use crate::theme::SPACE_3;

pub(super) const LABEL_MIN_W: f32 = 120.0;
pub(super) const ROW_GAP: f32 = SPACE_3;
pub(super) const SWITCH_W: f32 = 36.0;
pub(super) const BUTTON_PAD: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowShape {
    Beside,
    Stacked,
}

#[must_use]
pub(super) fn row_shape(card_inner: f32, label_w: f32, control_w: f32) -> RowShape {
    if label_w.max(LABEL_MIN_W) + ROW_GAP + control_w <= card_inner {
        RowShape::Beside
    } else {
        RowShape::Stacked
    }
}

#[must_use]
pub(super) fn control_room(shape: RowShape, card_inner: f32, label_w: f32) -> f32 {
    match shape {
        RowShape::Beside => (card_inner - label_w.max(LABEL_MIN_W) - ROW_GAP).max(0.0),
        RowShape::Stacked => card_inner.max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{fit, track_width, SegmentFit, Stretch};

    const GLYPH: f32 = 7.0;
    const EPSILON: f32 = 0.01;
    const LABELS: [&str; 5] = [
        "Thinking effort",
        "Permissions",
        "Model for its sessions",
        "Nachdenkaufwand für jede Ausführung",
        "Agent",
    ];
    const CONTROLS: [&[&str]; 3] = [
        &["Default", "Low", "Medium", "High", "Extra high", "Max"],
        &[
            "Default",
            "Ask before edits",
            "Accept edits",
            "Plan",
            "Auto",
        ],
        &["15m", "30m", "1h", "2h"],
    ];

    fn width(text: &str) -> f32 {
        u16::try_from(text.chars().count()).map_or(f32::MAX, f32::from) * GLYPH
    }

    fn widths(labels: &[&str]) -> Vec<f32> {
        labels.iter().map(|label| width(label)).collect()
    }

    #[test]
    fn rows_keep_their_label_whole_and_their_control_inside_the_card() {
        for card in (280_u16..=1000).step_by(20).map(f32::from) {
            for label in LABELS {
                let label_w = width(label);
                for control in CONTROLS {
                    let labels = widths(control);
                    let natural = track_width(&labels);
                    let shape = row_shape(card, label_w, natural);
                    let room = control_room(shape, card, label_w);
                    let beside_fits = label_w.max(LABEL_MIN_W) + ROW_GAP + natural <= card;
                    assert_eq!(shape == RowShape::Beside, beside_fits, "{card} {label}");
                    if shape == RowShape::Beside {
                        assert!(card - room - ROW_GAP + EPSILON >= label_w);
                        assert!(natural <= room + EPSILON);
                    }
                    assert!(room <= card + EPSILON);
                    match fit(&labels, &labels, room, Stretch::Natural) {
                        SegmentFit::Full(spans) | SegmentFit::Short(spans) => {
                            let right = spans.last().map_or(0.0, |span| span.right());
                            assert!(right <= room + EPSILON);
                        }
                        SegmentFit::Menu { width } => assert!(width <= room + EPSILON),
                    }
                }
                let switch = row_shape(card, label_w, SWITCH_W);
                assert_eq!(
                    switch == RowShape::Beside,
                    label_w.max(LABEL_MIN_W) + ROW_GAP + SWITCH_W <= card
                );
            }
        }
    }
}
