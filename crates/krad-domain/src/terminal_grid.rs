use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width_px: u16,
    pub cell_height_px: u16,
}

impl GridSize {
    #[must_use]
    pub const fn pixels(self) -> (u16, u16) {
        (
            self.cols.saturating_mul(self.cell_width_px),
            self.rows.saturating_mul(self.cell_height_px),
        )
    }
}
