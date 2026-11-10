use crate::theme::{SPACE_3, SPACE_5, SPACE_6};

pub(super) const NARROW_BELOW: f32 = 720.0;
pub(super) const MEDIUM_BELOW: f32 = 1180.0;
pub(super) const WIDE_BELOW: f32 = 1880.0;
pub(super) const MAX_CONTENT_W: f32 = 2240.0;
pub(super) const FALLBACK_W: f32 = 960.0;
const AVATAR_NARROW: f32 = 72.0;
const AVATAR_MEDIUM: f32 = 88.0;
const AVATAR_WIDE: f32 = 112.0;
pub(super) const CARD_PAD: f32 = SPACE_5;
pub(super) const HEAT_WEEKDAY_W: f32 = 28.0;
pub(super) const HEAT_GAP: f32 = 3.0;
pub(super) const HEAT_MIN_CELL: f32 = 9.0;
pub(super) const HEAT_MAX_CELL: f32 = 20.0;
pub(super) const MIN_TUNING_CARD_W: f32 = 640.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WidthClass {
    Narrow,
    Medium,
    Wide,
    Ultra,
}

impl WidthClass {
    #[must_use]
    pub(super) fn of(width: f32) -> Self {
        if width < NARROW_BELOW {
            Self::Narrow
        } else if width < MEDIUM_BELOW {
            Self::Medium
        } else if width < WIDE_BELOW {
            Self::Wide
        } else {
            Self::Ultra
        }
    }

    const fn columns(self) -> usize {
        match self {
            Self::Narrow => 1,
            Self::Medium => 2,
            Self::Wide => 3,
            Self::Ultra => 4,
        }
    }

    const fn tuning_columns(self) -> usize {
        match self {
            Self::Narrow => 1,
            Self::Medium | Self::Wide => 2,
            Self::Ultra => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HeroShape {
    Stacked,
    Beside,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HeatLayout {
    pub(super) cell: f32,
    pub(super) width: f32,
    pub(super) scrolls: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PageLayout {
    pub(super) class: WidthClass,
    pub(super) padding: f32,
    pub(super) content_w: f32,
    pub(super) gutter: f32,
    pub(super) columns: usize,
    pub(super) tuning_columns: usize,
    pub(super) hero: HeroShape,
    pub(super) avatar: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CardSpan {
    One,
    Fill,
    Full,
}

#[must_use]
pub(super) fn page_layout(container_w: f32) -> PageLayout {
    let container_w = if container_w.is_finite() && container_w > 0.0 {
        container_w
    } else {
        FALLBACK_W
    };
    let class = WidthClass::of(container_w);
    let padding = if class == WidthClass::Narrow {
        SPACE_3
    } else {
        SPACE_6
    };
    let content_w = (container_w - 2.0 * padding).clamp(0.0, MAX_CONTENT_W);
    PageLayout {
        class,
        padding,
        content_w,
        gutter: gutter_of(class),
        columns: class.columns(),
        tuning_columns: fitting_columns(
            content_w,
            gutter_of(class),
            MIN_TUNING_CARD_W,
            class.tuning_columns(),
        ),
        hero: if class == WidthClass::Narrow {
            HeroShape::Stacked
        } else {
            HeroShape::Beside
        },
        avatar: match class {
            WidthClass::Narrow => AVATAR_NARROW,
            WidthClass::Medium => AVATAR_MEDIUM,
            WidthClass::Wide | WidthClass::Ultra => AVATAR_WIDE,
        },
    }
}

impl PageLayout {
    #[must_use]
    pub(super) fn column_w(&self, columns: usize) -> f32 {
        let columns = columns.max(1);
        let gaps = count(columns - 1) * self.gutter;
        ((self.content_w - gaps) / count(columns)).max(0.0)
    }

    #[must_use]
    pub(super) fn span_w(&self, span: usize, columns: usize) -> f32 {
        let span = span.clamp(1, columns.max(1));
        self.column_w(columns) * count(span) + count(span - 1) * self.gutter
    }

    #[must_use]
    pub(super) fn heat(&self, weeks: usize) -> HeatLayout {
        let room = (self.content_w - 2.0 * CARD_PAD - HEAT_WEEKDAY_W).max(0.0);
        let weeks_f = count(weeks.max(1));
        let fitted = (room + HEAT_GAP) / weeks_f - HEAT_GAP;
        let cell = fitted.clamp(HEAT_MIN_CELL, HEAT_MAX_CELL);
        let width = HEAT_WEEKDAY_W + weeks_f * (cell + HEAT_GAP) - HEAT_GAP;
        HeatLayout {
            cell,
            width,
            scrolls: width > room + HEAT_WEEKDAY_W,
        }
    }
}

fn gutter_of(class: WidthClass) -> f32 {
    if class == WidthClass::Narrow {
        SPACE_3
    } else {
        SPACE_5
    }
}

#[must_use]
pub(super) fn fitting_columns(content_w: f32, gutter: f32, min_card: f32, most: usize) -> usize {
    (1..=most.max(1))
        .rev()
        .find(|columns| {
            let columns_f = count(*columns);
            (content_w - count(columns - 1) * gutter) / columns_f >= min_card
        })
        .unwrap_or(1)
}

#[must_use]
pub(super) fn place(spans: &[CardSpan], columns: usize) -> Vec<usize> {
    let columns = columns.max(1);
    let mut used = 0;
    spans
        .iter()
        .map(|span| {
            let taken = match span {
                CardSpan::One => 1,
                CardSpan::Full => columns,
                CardSpan::Fill if used == 0 => columns,
                CardSpan::Fill => columns - used,
            };
            if used + taken > columns {
                used = 0;
            }
            used = (used + taken) % columns;
            taken
        })
        .collect()
}

fn count(value: usize) -> f32 {
    u16::try_from(value).map_or(f32::from(u16::MAX), f32::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTHS: [f32; 5] = [640.0, 900.0, 1280.0, 1920.0, 2560.0];
    const HALF_YEAR_WEEKS: usize = 27;
    const OVERVIEW: [CardSpan; 5] = [
        CardSpan::Full,
        CardSpan::One,
        CardSpan::One,
        CardSpan::One,
        CardSpan::Fill,
    ];

    #[test]
    fn every_width_gets_its_own_class_columns_and_hero() {
        let shapes: Vec<(WidthClass, usize, usize, HeroShape)> = WIDTHS
            .map(page_layout)
            .iter()
            .map(|layout| {
                (
                    layout.class,
                    layout.columns,
                    layout.tuning_columns,
                    layout.hero,
                )
            })
            .collect();
        assert_eq!(
            shapes,
            vec![
                (WidthClass::Narrow, 1, 1, HeroShape::Stacked),
                (WidthClass::Medium, 2, 1, HeroShape::Beside),
                (WidthClass::Wide, 3, 1, HeroShape::Beside),
                (WidthClass::Ultra, 4, 2, HeroShape::Beside),
                (WidthClass::Ultra, 4, 3, HeroShape::Beside),
            ]
        );
    }

    #[test]
    fn content_fits_the_container_and_stops_growing_on_ultrawide() {
        for width in WIDTHS {
            let layout = page_layout(width);
            assert!(layout.content_w + 2.0 * layout.padding <= width + f32::EPSILON);
            assert!(layout.content_w <= MAX_CONTENT_W);
            let row: f32 = (0..layout.columns)
                .map(|_| layout.column_w(layout.columns))
                .sum::<f32>()
                + count(layout.columns - 1) * layout.gutter;
            assert!((row - layout.content_w).abs() < 0.01, "{width}");
            assert!(
                (layout.span_w(layout.columns, layout.columns) - layout.content_w).abs() < 0.01
            );
        }
        assert!((page_layout(4000.0).content_w - MAX_CONTENT_W).abs() < f32::EPSILON);
    }

    #[test]
    fn the_heat_map_grows_its_cells_and_scrolls_only_when_squeezed() {
        let heats: Vec<HeatLayout> = WIDTHS
            .map(|width| page_layout(width).heat(HALF_YEAR_WEEKS))
            .to_vec();
        assert!(heats.windows(2).all(|pair| pair[0].cell <= pair[1].cell));
        assert!(heats
            .iter()
            .all(|heat| (HEAT_MIN_CELL..=HEAT_MAX_CELL).contains(&heat.cell)));
        assert!(!heats[0].scrolls, "a 640 px window still fits half a year");
        assert!(page_layout(360.0).heat(HALF_YEAR_WEEKS).scrolls);
        assert!((heats[4].cell - HEAT_MAX_CELL).abs() < f32::EPSILON);
    }

    #[test]
    fn tuning_cards_never_get_narrower_than_their_minimum() {
        for window in (640_u16..=2560).step_by(40).map(f32::from) {
            let layout = page_layout(window);
            let width = layout.column_w(layout.tuning_columns);
            assert!(
                width >= MIN_TUNING_CARD_W || layout.tuning_columns == 1,
                "{window}: {} columns of {width}",
                layout.tuning_columns
            );
            let more = layout.tuning_columns + 1;
            assert!(
                more > layout.class.tuning_columns() || layout.column_w(more) < MIN_TUNING_CARD_W,
                "{window}"
            );
        }
        assert_eq!(page_layout(1280.0).tuning_columns, 1);
        assert_eq!(page_layout(1920.0).tuning_columns, 2);
        assert_eq!(page_layout(2560.0).tuning_columns, 3);
    }

    #[test]
    fn cards_reflow_by_columns_and_the_last_one_fills_its_row() {
        assert_eq!(place(&OVERVIEW, 1), vec![1, 1, 1, 1, 1]);
        assert_eq!(place(&OVERVIEW, 2), vec![2, 1, 1, 1, 1]);
        assert_eq!(place(&OVERVIEW, 3), vec![3, 1, 1, 1, 3]);
        assert_eq!(place(&OVERVIEW, 4), vec![4, 1, 1, 1, 1]);
        assert_eq!(
            place(&[CardSpan::One, CardSpan::Full, CardSpan::One], 3),
            vec![1, 3, 1]
        );
    }

    #[test]
    fn a_container_not_measured_yet_gets_a_sensible_layout() {
        assert_eq!(page_layout(0.0), page_layout(FALLBACK_W));
        assert_eq!(page_layout(f32::NAN), page_layout(FALLBACK_W));
    }
}
