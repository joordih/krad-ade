const SEGMENT_PAD_X: f32 = 10.0;
pub(crate) const SEGMENT_GAP: f32 = 2.0;
pub(crate) const TRACK_PAD: f32 = crate::theme::CAPSULE_PAD;
pub(crate) const MENU_BELOW: f32 = 240.0;
const MENU_CHEVRON: f32 = 22.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Span {
    pub(crate) left: f32,
    pub(crate) width: f32,
}

impl Span {
    #[must_use]
    pub(crate) fn right(self) -> f32 {
        self.left + self.width
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stretch {
    Natural,
    Fill,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SegmentFit {
    Full(Vec<Span>),
    Short(Vec<Span>),
    Menu { width: f32 },
}

#[must_use]
fn segment_need(label: f32) -> f32 {
    label.max(0.0) + 2.0 * SEGMENT_PAD_X
}

#[must_use]
pub(crate) fn natural_width(labels: &[f32]) -> f32 {
    let gaps = count(labels.len().saturating_sub(1)) * SEGMENT_GAP;
    labels.iter().copied().map(segment_need).sum::<f32>() + gaps
}

#[must_use]
pub(crate) fn track_width(labels: &[f32]) -> f32 {
    natural_width(labels) + 2.0 * TRACK_PAD
}

#[must_use]
pub(crate) fn spans(labels: &[f32], inner: f32, stretch: Stretch) -> Option<Vec<Span>> {
    let natural = natural_width(labels);
    if labels.is_empty() || natural > inner {
        return None;
    }
    let scale = match stretch {
        Stretch::Natural => 1.0,
        Stretch::Fill => {
            let needs = natural - count(labels.len() - 1) * SEGMENT_GAP;
            (inner - count(labels.len() - 1) * SEGMENT_GAP) / needs
        }
    };
    let mut left = 0.0;
    Some(
        labels
            .iter()
            .map(|label| {
                let span = Span {
                    left,
                    width: segment_need(*label) * scale,
                };
                left = span.right() + SEGMENT_GAP;
                span
            })
            .collect(),
    )
}

#[must_use]
pub(crate) fn fit(full: &[f32], short: &[f32], container: f32, stretch: Stretch) -> SegmentFit {
    let inner = (container - 2.0 * TRACK_PAD).max(0.0);
    if container >= MENU_BELOW {
        if let Some(spans) = spans(full, inner, stretch) {
            return SegmentFit::Full(spans);
        }
        if let Some(spans) = spans(short, inner, stretch) {
            return SegmentFit::Short(spans);
        }
    }
    let widest = full.iter().copied().fold(0.0, f32::max);
    SegmentFit::Menu {
        width: (segment_need(widest) + MENU_CHEVRON).min(container.max(0.0)),
    }
}

fn count(value: usize) -> f32 {
    u16::try_from(value).map_or(f32::from(u16::MAX), f32::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLYPH: f32 = 7.0;
    const EPSILON: f32 = 0.01;
    const SETS: [&[&str]; 7] = [
        &["Runs", "Lines changed", "Tokens", "Cost"],
        &["At a glance", "Compass", "Keepsakes", "Tuning"],
        &["Ausführungen", "Geänderte Zeilen", "Tokens", "Kosten"],
        &[
            "Suoritukset",
            "Muutetut rivit",
            "Tunnisteet",
            "Kustannukset",
        ],
        &["Vue d'ensemble", "Boussole", "Souvenirs", "Réglages"],
        &["実行", "変更行", "トークン", "費用"],
        &["On", "Off"],
    ];

    fn measured(labels: &[&str]) -> Vec<f32> {
        labels
            .iter()
            .map(|label| count(label.chars().count()) * GLYPH)
            .collect()
    }

    fn shortened(labels: &[&str]) -> Vec<f32> {
        labels
            .iter()
            .map(|label| count(label.chars().count().min(6)) * GLYPH)
            .collect()
    }

    fn check(spans: &[Span], labels: &[f32], inner: f32) {
        assert_eq!(spans.len(), labels.len());
        for (span, label) in spans.iter().zip(labels) {
            assert!(span.left >= -EPSILON && span.right() <= inner + EPSILON);
            assert!(span.width + EPSILON >= label + 2.0 * SEGMENT_PAD_X);
        }
        for pair in spans.windows(2) {
            assert!(pair[0].right() <= pair[1].left + EPSILON, "{pair:?}");
        }
    }

    #[test]
    fn every_width_gets_disjoint_segments_wide_enough_for_their_labels() {
        for set in SETS {
            let full = measured(set);
            let short = shortened(set);
            for container in (320_u16..=1600).step_by(40).map(f32::from) {
                let inner = container - 2.0 * TRACK_PAD;
                for stretch in [Stretch::Natural, Stretch::Fill] {
                    match fit(&full, &short, container, stretch) {
                        SegmentFit::Full(spans) => check(&spans, &full, inner),
                        SegmentFit::Short(spans) => {
                            assert!(natural_width(&full) > inner);
                            check(&spans, &short, inner);
                        }
                        SegmentFit::Menu { width } => {
                            assert!(natural_width(&short) > inner);
                            assert!(width <= container + EPSILON);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn filling_shares_the_room_and_natural_keeps_each_label_size() {
        let labels = measured(SETS[0]);
        let natural = natural_width(&labels);
        let filled = spans(&labels, natural * 2.0, Stretch::Fill).unwrap_or_default();
        let last = filled.last().map_or(0.0, |span| span.right());
        assert!((last - natural * 2.0).abs() < EPSILON);
        let kept = spans(&labels, natural * 2.0, Stretch::Natural).unwrap_or_default();
        assert!((kept[1].width - segment_need(labels[1])).abs() < EPSILON);
        assert!(spans(&labels, natural - 1.0, Stretch::Natural).is_none());
    }

    #[test]
    fn a_very_narrow_container_falls_back_to_a_menu() {
        let labels = measured(SETS[1]);
        assert!(matches!(
            fit(&labels, &labels, MENU_BELOW - 1.0, Stretch::Fill),
            SegmentFit::Menu { .. }
        ));
    }
}
