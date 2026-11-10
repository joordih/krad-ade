use crate::theme::{SPACE_3, SPACE_5};

pub(super) const STACK_BELOW: f32 = 760.0;
pub(super) const PAIR_BELOW: f32 = 1240.0;
pub(super) const ROOMY_FROM: f32 = 2000.0;
const SIDE_SHARE: f32 = 0.34;
const TRIO_SIDE_SHARE: f32 = 0.24;
const MIN_SIDE_W: f32 = 260.0;
const MAX_SIDE_W: f32 = 420.0;
const MIN_DIGEST_CARD_W: f32 = 320.0;
const MAX_DIGEST_COLUMNS: usize = 3;
const DEFAULT_DIGEST_COLUMNS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AreaShape {
    Stacked,
    Paired,
    Trio,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AreaLayout {
    pub(super) shape: AreaShape,
    pub(super) gap: f32,
    pub(super) chats_w: f32,
    pub(super) digests_w: f32,
    pub(super) sharing_w: f32,
    pub(super) digest_columns: usize,
    pub(super) digest_card_w: f32,
}

fn side_width(container: f32, share: f32) -> f32 {
    (container * share).clamp(MIN_SIDE_W, MAX_SIDE_W)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn digest_grid(width: f32, gap: f32, roomy: bool) -> (usize, f32) {
    let wanted = if roomy {
        MAX_DIGEST_COLUMNS
    } else {
        DEFAULT_DIGEST_COLUMNS
    };
    let fits = ((width + gap) / (MIN_DIGEST_CARD_W + gap)).floor().max(1.0) as usize;
    let columns = fits.min(wanted).max(1);
    let card = (width - gap * (columns as f32 - 1.0)) / columns as f32;
    (columns, card.max(0.0))
}

#[must_use]
pub(super) fn area_layout(container: f32) -> AreaLayout {
    let container = if container.is_finite() {
        container.max(0.0)
    } else {
        0.0
    };
    let gap = if container < STACK_BELOW {
        SPACE_3
    } else {
        SPACE_5
    };
    let (shape, chats_w, digests_w, sharing_w) = if container < STACK_BELOW {
        (AreaShape::Stacked, container, container, container)
    } else if container < PAIR_BELOW {
        let side = side_width(container, SIDE_SHARE);
        (AreaShape::Paired, side, container - side - gap, side)
    } else {
        let side = side_width(container, TRIO_SIDE_SHARE);
        let middle = container - 2.0 * (side + gap);
        (AreaShape::Trio, side, middle, side)
    };
    let (digest_columns, digest_card_w) = digest_grid(digests_w, gap, container >= ROOMY_FROM);
    AreaLayout {
        shape,
        gap,
        chats_w,
        digests_w,
        sharing_w,
        digest_columns,
        digest_card_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTHS: [f32; 5] = [640.0, 900.0, 1280.0, 1920.0, 2560.0];
    const EPSILON: f32 = 0.01;

    fn row_width(layout: &AreaLayout) -> f32 {
        match layout.shape {
            AreaShape::Stacked => layout.digests_w,
            AreaShape::Paired => layout.chats_w + layout.gap + layout.digests_w,
            AreaShape::Trio => {
                layout.chats_w + layout.digests_w + layout.sharing_w + 2.0 * layout.gap
            }
        }
    }

    #[test]
    fn each_width_picks_its_shape() {
        let shapes: Vec<AreaShape> = WIDTHS
            .iter()
            .map(|width| area_layout(*width).shape)
            .collect();
        assert_eq!(
            shapes,
            [
                AreaShape::Stacked,
                AreaShape::Paired,
                AreaShape::Trio,
                AreaShape::Trio,
                AreaShape::Trio
            ]
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn nothing_overflows_or_collapses_at_any_width() {
        for width in WIDTHS {
            let layout = area_layout(width);
            assert!(
                (row_width(&layout) - width).abs() < EPSILON,
                "{width}: {layout:?}"
            );
            assert!(layout.chats_w >= MIN_SIDE_W.min(width), "{width}");
            assert!(layout.digests_w >= MIN_DIGEST_CARD_W, "{width}: {layout:?}");
            let cards = layout.digest_card_w * layout.digest_columns as f32
                + layout.gap * (layout.digest_columns as f32 - 1.0);
            assert!(cards <= layout.digests_w + EPSILON, "{width}");
            assert!(
                layout.digest_card_w >= MIN_DIGEST_CARD_W - EPSILON || layout.digest_columns == 1
            );
        }
    }

    #[test]
    fn digest_cards_spread_out_as_the_page_grows() {
        let columns: Vec<usize> = WIDTHS
            .iter()
            .map(|width| area_layout(*width).digest_columns)
            .collect();
        assert_eq!(columns, [1, 1, 1, 2, 3]);
        assert!(columns.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn a_width_not_yet_measured_does_not_break_the_layout() {
        let layout = area_layout(f32::NAN);
        assert_eq!(layout.shape, AreaShape::Stacked);
        assert_eq!(layout.digest_columns, 1);
        assert!(layout.digest_card_w >= 0.0);
    }
}
