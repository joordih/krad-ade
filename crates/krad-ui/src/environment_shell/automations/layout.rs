use super::grid::{fit_grid, GridFit, GridSpec};
use crate::theme::{SPACE_3, SPACE_6};

pub(super) const PAGE_PAD: f32 = SPACE_6;
const CARD_GAP: f32 = SPACE_3;
const SIDE_BY_SIDE_FROM: f32 = 1180.0;
const CARD_MIN_W: f32 = 300.0;
const CARD_MAX_W: f32 = 520.0;
const CARD_H: f32 = 148.0;
const ROW_H: f32 = 44.0;
const MAX_CARD_COLUMNS: usize = 6;
const TILE_MIN_W: f32 = 260.0;
const TILE_MAX_W: f32 = 380.0;
const TILE_H: f32 = 156.0;
const MAX_TILE_COLUMNS: usize = 8;
pub(super) const SHELF_PAD: f32 = SPACE_3;
const SHELF_BORDER: f32 = 1.0;
pub(super) const CARD_GRID: GridSpec = GridSpec {
    min_cell: CARD_MIN_W,
    max_cell: CARD_MAX_W,
    gap: CARD_GAP,
    max_columns: MAX_CARD_COLUMNS,
    cell_h: CARD_H,
};
pub(super) const TILE_GRID: GridSpec = GridSpec {
    min_cell: TILE_MIN_W,
    max_cell: TILE_MAX_W,
    gap: CARD_GAP,
    max_columns: MAX_TILE_COLUMNS,
    cell_h: TILE_H,
};
const BOARD_SHARE: f32 = 0.38;
const BOARD_MIN_W: f32 = 340.0;
const BOARD_MAX_W: f32 = 560.0;
pub(super) const EDITOR_MAX_W: f32 = 780.0;
const BOARD_MAX_W_ALONE: f32 = 1680.0;
const RAIL_HIDDEN_BELOW: f32 = 560.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Density {
    Cards,
    Rows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BoardRole {
    Alone,
    Beside,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Arrangement {
    Board(BoardFit),
    Editor {
        width: f32,
        rail: bool,
    },
    SideBySide {
        board: BoardFit,
        editor_w: f32,
        rail: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BoardFit {
    pub(super) width: f32,
    pub(super) inner: f32,
    pub(super) density: Density,
}

impl BoardFit {
    #[must_use]
    pub(super) fn cards(self, items: usize) -> GridFit {
        let spec = match self.density {
            Density::Cards => CARD_GRID,
            Density::Rows => GridSpec {
                max_columns: 1,
                max_cell: self.inner,
                min_cell: 0.0,
                cell_h: ROW_H,
                ..CARD_GRID
            },
        };
        fit_grid(self.inner, items, spec)
    }

    #[must_use]
    pub(super) fn shelf_inner(self) -> f32 {
        (self.inner - 2.0 * (SHELF_PAD + SHELF_BORDER)).max(0.0)
    }

    #[must_use]
    pub(super) fn tiles(self, items: usize) -> GridFit {
        fit_grid(self.shelf_inner(), items, TILE_GRID)
    }
}

fn inner_width(width: f32) -> f32 {
    (width.min(BOARD_MAX_W_ALONE + 2.0 * PAGE_PAD) - 2.0 * PAGE_PAD).max(0.0)
}

#[must_use]
pub(super) fn board_fit(width: f32, role: BoardRole) -> BoardFit {
    let inner = inner_width(width);
    let density = match role {
        BoardRole::Alone => Density::Cards,
        BoardRole::Beside => Density::Rows,
    };
    BoardFit {
        width,
        inner,
        density,
    }
}

#[must_use]
pub(super) fn can_sit_side_by_side(width: f32) -> bool {
    width >= SIDE_BY_SIDE_FROM
}

#[must_use]
fn board_side_width(width: f32) -> f32 {
    (width * BOARD_SHARE).clamp(BOARD_MIN_W, BOARD_MAX_W)
}

#[must_use]
pub(super) fn arrangement(width: f32, editing: bool, side_by_side: bool) -> Arrangement {
    if !editing {
        return Arrangement::Board(board_fit(width, BoardRole::Alone));
    }
    if side_by_side && can_sit_side_by_side(width) {
        let board_w = board_side_width(width);
        let editor_w = width - board_w;
        return Arrangement::SideBySide {
            board: board_fit(board_w, BoardRole::Beside),
            editor_w,
            rail: editor_w >= RAIL_HIDDEN_BELOW,
        };
    }
    Arrangement::Editor {
        width,
        rail: width >= RAIL_HIDDEN_BELOW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTHS: [f32; 5] = [640.0, 900.0, 1280.0, 1920.0, 2560.0];

    fn alone(width: f32) -> BoardFit {
        board_fit(width, BoardRole::Alone)
    }

    #[test]
    fn the_board_grows_columns_with_the_width_and_never_overflows() {
        let columns: Vec<usize> = WIDTHS.iter().map(|w| alone(*w).cards(6).columns).collect();
        assert_eq!(columns, [1, 2, 3, 5, 5]);
        for width in (480_u16..=3000)
            .step_by(40)
            .map(f32::from)
            .chain([1240.0, 1900.0])
        {
            for role in [BoardRole::Alone, BoardRole::Beside] {
                let fit = board_fit(width, role);
                assert!(fit.inner + 2.0 * PAGE_PAD <= width + 0.01, "{width}");
                assert!(fit.cards(7).width() <= fit.inner + 0.01, "{width} {role:?}");
                assert!(
                    fit.tiles(6).width() <= fit.shelf_inner() + 0.01,
                    "{width} {role:?}"
                );
                assert!(fit.shelf_inner() + 2.0 * (SHELF_PAD + SHELF_BORDER) <= fit.inner + 0.01);
                let tiles = fit.tiles(6);
                if tiles.columns > 1 {
                    assert!(tiles.width() >= 0.9 * fit.shelf_inner(), "{width} {role:?}");
                }
                let cards = fit.cards(7);
                if cards.columns > 1 {
                    assert!(cards.width() >= 0.9 * fit.inner, "{width} {role:?}");
                }
            }
        }
    }

    #[test]
    fn a_board_beside_the_editor_reads_as_rows_and_alone_as_cards() {
        assert_eq!(board_fit(480.0, BoardRole::Beside).density, Density::Rows);
        assert_eq!(board_fit(480.0, BoardRole::Beside).cards(4).columns, 1);
        assert_eq!(alone(420.0).density, Density::Cards);
        assert_eq!(alone(420.0).cards(4).columns, 1);
        assert_eq!(alone(1280.0).density, Density::Cards);
    }

    #[test]
    fn the_blueprint_shelf_adds_tiles_as_room_appears() {
        let tiles: Vec<usize> = WIDTHS.iter().map(|w| alone(*w).tiles(6).columns).collect();
        assert_eq!(tiles, [2, 3, 4, 6, 6]);
        assert!(tiles.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn editing_stacks_unless_the_user_asks_for_both_and_there_is_room() {
        for width in WIDTHS {
            assert!(matches!(
                arrangement(width, true, false),
                Arrangement::Editor { .. }
            ));
            assert!(matches!(
                arrangement(width, false, true),
                Arrangement::Board(_)
            ));
        }
        let side: Vec<bool> = WIDTHS
            .iter()
            .map(|w| matches!(arrangement(*w, true, true), Arrangement::SideBySide { .. }))
            .collect();
        assert_eq!(side, [false, false, true, true, true]);
    }

    #[test]
    fn side_by_side_shares_the_width_and_keeps_the_board_readable() {
        for width in [1280.0, 1920.0, 2560.0] {
            let Arrangement::SideBySide {
                board,
                editor_w,
                rail,
            } = arrangement(width, true, true)
            else {
                unreachable!()
            };
            assert!((board.width + editor_w - width).abs() < f32::EPSILON);
            assert!((BOARD_MIN_W..=BOARD_MAX_W).contains(&board.width));
            assert_eq!(board.cards(3).columns, 1);
            assert!(editor_w >= EDITOR_MAX_W.min(width - BOARD_MAX_W));
            assert!(rail);
        }
    }

    #[test]
    fn the_step_rail_hides_when_the_editor_is_tight() {
        assert_eq!(
            arrangement(480.0, true, false),
            Arrangement::Editor {
                width: 480.0,
                rail: false
            }
        );
        assert_eq!(
            arrangement(640.0, true, true),
            Arrangement::Editor {
                width: 640.0,
                rail: true
            }
        );
    }
}
