pub(super) const NARROW_MAX_W: f32 = 760.0;
pub(super) const WIDE_MIN_W: f32 = 1180.0;
const LIST_FRACTION: f32 = 0.24;
const LIST_MIN_W: f32 = 248.0;
const LIST_MAX_W: f32 = 380.0;
const FILES_FRACTION: f32 = 0.24;
const FILES_MIN_W: f32 = 168.0;
const FILES_MAX_W: f32 = 340.0;
const FILES_RAIL_MIN_DETAIL_W: f32 = 560.0;
const SIDE_FRACTION: f32 = 0.3;
const SIDE_MIN_W: f32 = 260.0;
const SIDE_MAX_W: f32 = 360.0;
const SIDE_MIN_DETAIL_W: f32 = 860.0;
pub(super) const READING_MAX_W: f32 = 1040.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum WidthClass {
    Narrow,
    Medium,
    #[default]
    Wide,
}

impl WidthClass {
    pub(super) fn of(width: f32) -> Self {
        if width < NARROW_MAX_W {
            Self::Narrow
        } else if width < WIDE_MIN_W {
            Self::Medium
        } else {
            Self::Wide
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(super) struct ReviewFrame {
    pub(super) class: WidthClass,
    pub(super) list_w: Option<f32>,
    pub(super) detail_w: f32,
    pub(super) files_w: Option<f32>,
    pub(super) side_w: Option<f32>,
}

impl ReviewFrame {
    pub(super) const fn shows_detail(&self) -> bool {
        self.detail_w > 0.0
    }

    pub(super) fn needs_back(&self) -> bool {
        self.class == WidthClass::Narrow
    }
}

fn share(width: f32, fraction: f32, min: f32, max: f32) -> f32 {
    (width * fraction).clamp(min, max)
}

pub(super) fn review_frame(width: f32, detail_open: bool) -> ReviewFrame {
    let width = width.max(0.0);
    let class = WidthClass::of(width);
    let (list_w, detail_w) = match (class, detail_open) {
        (WidthClass::Narrow, false) => (Some(width), 0.0),
        (WidthClass::Narrow, true) => (None, width),
        (WidthClass::Medium, _) => (Some(LIST_MIN_W), width - LIST_MIN_W),
        (WidthClass::Wide, _) => {
            let list = share(width, LIST_FRACTION, LIST_MIN_W, LIST_MAX_W);
            (Some(list), width - list)
        }
    };
    let detail_w = detail_w.max(0.0);
    let files_w = (detail_w >= FILES_RAIL_MIN_DETAIL_W)
        .then(|| share(detail_w, FILES_FRACTION, FILES_MIN_W, FILES_MAX_W));
    let reading = detail_w.min(READING_MAX_W + SIDE_MAX_W);
    let side_w = (detail_w >= SIDE_MIN_DETAIL_W)
        .then(|| share(reading, SIDE_FRACTION, SIDE_MIN_W, SIDE_MAX_W));
    ReviewFrame {
        class,
        list_w,
        detail_w,
        files_w,
        side_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTHS: [f32; 5] = [640.0, 900.0, 1280.0, 1920.0, 2560.0];
    const LAYOUT_TOLERANCE_PX: f32 = 0.01;

    #[test]
    fn each_width_falls_in_its_class() {
        let classes = WIDTHS.map(WidthClass::of);
        assert_eq!(
            classes,
            [
                WidthClass::Narrow,
                WidthClass::Medium,
                WidthClass::Wide,
                WidthClass::Wide,
                WidthClass::Wide
            ]
        );
    }

    #[test]
    fn list_and_detail_always_fill_the_width_without_overflowing() {
        for width in WIDTHS {
            for open in [false, true] {
                let frame = review_frame(width, open);
                let used = frame.list_w.unwrap_or_default() + frame.detail_w;
                assert!(
                    (used - width).abs() <= LAYOUT_TOLERANCE_PX,
                    "{width} {open}"
                );
                if let Some(files) = frame.files_w {
                    assert!(files < frame.detail_w, "{width}");
                }
            }
        }
    }

    #[test]
    fn a_narrow_review_shows_either_the_list_or_the_detail() {
        let browsing = review_frame(640.0, false);
        assert_eq!(
            (browsing.list_w, browsing.shows_detail()),
            (Some(640.0), false)
        );
        let reading = review_frame(640.0, true);
        assert_eq!((reading.list_w, reading.detail_w), (None, 640.0));
        assert!(reading.needs_back());
        assert_eq!(
            reading.side_w, None,
            "the side cards stack under the description"
        );
        assert_eq!(reading.files_w, Some(FILES_MIN_W));
    }

    #[test]
    fn a_medium_review_keeps_a_compact_list_and_stacks_the_side_cards() {
        let frame = review_frame(900.0, true);
        assert_eq!(frame.list_w, Some(LIST_MIN_W));
        assert_eq!(frame.side_w, None);
        assert!(!frame.needs_back());
        assert_eq!(frame.files_w, Some(FILES_MIN_W));
    }

    #[test]
    fn wide_reviews_grow_the_columns_up_to_their_caps() {
        let at = |width| review_frame(width, true);
        assert_eq!(at(1280.0).list_w, Some(1280.0 * LIST_FRACTION));
        assert!(at(1280.0)
            .side_w
            .is_some_and(|side| side > SIDE_MIN_W && side < SIDE_MAX_W));
        assert_eq!(at(1920.0).list_w, Some(LIST_MAX_W));
        assert_eq!(at(2560.0).list_w, Some(LIST_MAX_W));
        assert_eq!(at(2560.0).files_w, Some(FILES_MAX_W));
        assert_eq!(at(2560.0).side_w, Some(SIDE_MAX_W));
    }
}
