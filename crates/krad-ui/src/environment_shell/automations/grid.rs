use gpui_kit::{div, px, AnyElement, Div, IntoElement, ParentElement, Styled};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GridSpec {
    pub(super) min_cell: f32,
    pub(super) max_cell: f32,
    pub(super) gap: f32,
    pub(super) max_columns: usize,
    pub(super) cell_h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GridFit {
    pub(super) columns: usize,
    pub(super) cell_w: f32,
    pub(super) cell_h: f32,
    pub(super) gap: f32,
    pub(super) rows: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CellRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

fn count(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn whole(value: f32) -> usize {
    (1_u16..u16::MAX)
        .take_while(|step| f32::from(*step) <= value)
        .last()
        .map_or(0, usize::from)
}

#[must_use]
pub(super) fn column_count(container: f32, spec: GridSpec) -> usize {
    whole((container.max(0.0) + spec.gap) / (spec.min_cell + spec.gap))
        .clamp(1, spec.max_columns.max(1))
}

#[must_use]
pub(super) fn fit_grid(container: f32, items: usize, spec: GridSpec) -> GridFit {
    let container = container.max(0.0);
    let columns = column_count(container, spec);
    let spare = container - count(columns - 1) * spec.gap;
    GridFit {
        columns,
        cell_w: (spare / count(columns)).min(spec.max_cell).max(0.0),
        cell_h: spec.cell_h,
        gap: spec.gap,
        rows: items.div_ceil(columns),
    }
}

impl GridFit {
    #[must_use]
    pub(super) fn width(self) -> f32 {
        count(self.columns) * self.cell_w + count(self.columns.saturating_sub(1)) * self.gap
    }

    #[must_use]
    pub(super) fn height(self) -> f32 {
        count(self.rows) * self.cell_h + count(self.rows.saturating_sub(1)) * self.gap
    }

    #[must_use]
    pub(super) fn cell(self, index: usize) -> CellRect {
        CellRect {
            x: count(index % self.columns) * (self.cell_w + self.gap),
            y: count(index / self.columns) * (self.cell_h + self.gap),
            w: self.cell_w,
            h: self.cell_h,
        }
    }
}

pub(super) fn grid_rows(fit: GridFit, cells: Vec<AnyElement>) -> Div {
    let mut cells = cells.into_iter();
    let rows: Vec<AnyElement> = (0..fit.rows)
        .map(|_| {
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_start()
                .gap(px(fit.gap))
                .children(cells.by_ref().take(fit.columns))
                .into_any_element()
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(fit.width()))
        .gap(px(fit.gap))
        .children(rows)
}

#[cfg(test)]
mod tests {
    use super::super::layout::{CARD_GRID, TILE_GRID};
    use super::*;

    const SPECS: [GridSpec; 2] = [CARD_GRID, TILE_GRID];
    const SCREENSHOT_WIDTHS: [f32; 3] = [640.0, 1240.0, 1900.0];
    const ITEMS: [usize; 5] = [1, 5, 6, 7, 17];
    const EPSILON: f32 = 0.01;
    const MIN_ROW_USE: f32 = 0.9;

    fn widths() -> impl Iterator<Item = f32> {
        (480_u16..=3000)
            .step_by(40)
            .map(f32::from)
            .chain(SCREENSHOT_WIDTHS)
    }

    fn overlaps(a: CellRect, b: CellRect) -> bool {
        a.x + a.w > b.x + EPSILON
            && b.x + b.w > a.x + EPSILON
            && a.y + a.h > b.y + EPSILON
            && b.y + b.h > a.y + EPSILON
    }

    #[test]
    fn the_column_count_follows_the_classic_formula() {
        let tiles = SPECS[1];
        assert_eq!(column_count(531.0, tiles), 1);
        assert_eq!(column_count(532.0, tiles), 2);
        assert_eq!(column_count(804.0, tiles), 3);
        assert_eq!(column_count(1654.0, tiles), 6);
        assert_eq!(column_count(10.0, tiles), 1);
        assert_eq!(column_count(9_000.0, tiles), tiles.max_columns);
    }

    #[test]
    fn every_width_gets_columns_that_fill_the_row_without_overflow() {
        for spec in SPECS {
            for width in widths() {
                let fit = fit_grid(width, 6, spec);
                assert!(fit.columns >= 1);
                assert!(fit.width() <= width + EPSILON, "{width}: {fit:?}");
                assert!(fit.cell_w <= spec.max_cell + EPSILON);
                if width >= spec.min_cell {
                    assert!(fit.cell_w >= spec.min_cell - EPSILON, "{width}: {fit:?}");
                }
                if fit.columns > 1 {
                    assert!(
                        fit.width() >= MIN_ROW_USE * width,
                        "{width}: {} of {width}",
                        fit.width()
                    );
                }
            }
        }
    }

    #[test]
    fn cells_share_one_size_never_overlap_and_stay_inside() {
        for spec in SPECS {
            for width in widths() {
                for items in ITEMS {
                    let fit = fit_grid(width, items, spec);
                    assert_eq!(fit.rows, items.div_ceil(fit.columns));
                    let cells: Vec<CellRect> = (0..items).map(|index| fit.cell(index)).collect();
                    for (at, cell) in cells.iter().enumerate() {
                        assert!((cell.w - fit.cell_w).abs() < EPSILON);
                        assert!((cell.h - spec.cell_h).abs() < EPSILON);
                        assert!(cell.x >= 0.0 && cell.x + cell.w <= width + EPSILON);
                        assert!(cell.y >= 0.0 && cell.y + cell.h <= fit.height() + EPSILON);
                        for other in &cells[at + 1..] {
                            assert!(
                                !overlaps(*cell, *other),
                                "{width} {items}: {cell:?} {other:?}"
                            );
                        }
                    }
                    let rows_seen = cells.iter().filter(|cell| cell.x.abs() < EPSILON).count();
                    assert_eq!(rows_seen, fit.rows);
                }
            }
        }
    }
}
