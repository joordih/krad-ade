pub const CELL_STEPS: u8 = 8;
const HALF: u8 = CELL_STEPS / 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellRect {
    pub left: u8,
    pub top: u8,
    pub right: u8,
    pub bottom: u8,
}

impl CellRect {
    const fn new(left: u8, top: u8, right: u8, bottom: u8) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

const FULL: CellRect = CellRect::new(0, 0, CELL_STEPS, CELL_STEPS);
const UPPER_LEFT: CellRect = CellRect::new(0, 0, HALF, HALF);
const UPPER_RIGHT: CellRect = CellRect::new(HALF, 0, CELL_STEPS, HALF);
const LOWER_LEFT: CellRect = CellRect::new(0, HALF, HALF, CELL_STEPS);
const LOWER_RIGHT: CellRect = CellRect::new(HALF, HALF, CELL_STEPS, CELL_STEPS);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shade {
    Solid,
    Light,
    Medium,
    Dark,
}

impl Shade {
    #[must_use]
    pub const fn opacity(self) -> f32 {
        match self {
            Self::Solid => 1.0,
            Self::Light => 0.25,
            Self::Medium => 0.5,
            Self::Dark => 0.75,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Block {
    pub rects: &'static [CellRect],
    pub shade: Shade,
}

const fn solid(rects: &'static [CellRect]) -> Block {
    Block {
        rects,
        shade: Shade::Solid,
    }
}

const fn shaded(shade: Shade) -> Block {
    Block {
        rects: &[FULL],
        shade,
    }
}

const fn lower(eighths: u8) -> CellRect {
    CellRect::new(0, CELL_STEPS - eighths, CELL_STEPS, CELL_STEPS)
}

const fn left(eighths: u8) -> CellRect {
    CellRect::new(0, 0, eighths, CELL_STEPS)
}

const FIRST_BLOCK: u32 = 0x2580;

const BLOCKS: [Block; 32] = [
    solid(&[CellRect::new(0, 0, CELL_STEPS, HALF)]),
    solid(&[lower(1)]),
    solid(&[lower(2)]),
    solid(&[lower(3)]),
    solid(&[lower(4)]),
    solid(&[lower(5)]),
    solid(&[lower(6)]),
    solid(&[lower(7)]),
    solid(&[FULL]),
    solid(&[left(7)]),
    solid(&[left(6)]),
    solid(&[left(5)]),
    solid(&[left(4)]),
    solid(&[left(3)]),
    solid(&[left(2)]),
    solid(&[left(1)]),
    solid(&[CellRect::new(HALF, 0, CELL_STEPS, CELL_STEPS)]),
    shaded(Shade::Light),
    shaded(Shade::Medium),
    shaded(Shade::Dark),
    solid(&[CellRect::new(0, 0, CELL_STEPS, 1)]),
    solid(&[CellRect::new(CELL_STEPS - 1, 0, CELL_STEPS, CELL_STEPS)]),
    solid(&[LOWER_LEFT]),
    solid(&[LOWER_RIGHT]),
    solid(&[UPPER_LEFT]),
    solid(&[UPPER_LEFT, LOWER_LEFT, LOWER_RIGHT]),
    solid(&[UPPER_LEFT, LOWER_RIGHT]),
    solid(&[UPPER_LEFT, UPPER_RIGHT, LOWER_LEFT]),
    solid(&[UPPER_LEFT, UPPER_RIGHT, LOWER_RIGHT]),
    solid(&[UPPER_RIGHT]),
    solid(&[UPPER_RIGHT, LOWER_LEFT]),
    solid(&[UPPER_RIGHT, LOWER_LEFT, LOWER_RIGHT]),
];

#[must_use]
pub fn block(character: char) -> Option<Block> {
    let offset = u32::from(character).checked_sub(FIRST_BLOCK)?;
    BLOCKS.get(usize::try_from(offset).ok()?).copied()
}

#[must_use]
pub fn is_blocks(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|character| block(character).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAST_BLOCK: u32 = 0x259F;

    fn shape(character: char) -> Block {
        block(character).unwrap_or_else(|| unreachable!("{character:?} is a block element"))
    }

    #[test]
    fn every_block_element_has_a_shape_inside_its_cell() {
        for code in FIRST_BLOCK..=LAST_BLOCK {
            let character =
                char::from_u32(code).unwrap_or_else(|| unreachable!("U+{code:04X} is a char"));
            for rect in shape(character).rects {
                assert!(rect.left < rect.right && rect.top < rect.bottom);
                assert!(rect.right <= CELL_STEPS && rect.bottom <= CELL_STEPS);
            }
        }
        assert_eq!(
            shape('█').rects,
            [CellRect::new(0, 0, CELL_STEPS, CELL_STEPS)],
            "the full block fills its cell"
        );
        assert_eq!(shape('▛').rects.len(), 3, "three quadrants");
        assert_eq!(shape('▒').shade, Shade::Medium);
    }

    #[test]
    fn characters_outside_the_range_are_not_blocks() {
        assert_eq!(block('a'), None);
        assert_eq!(block('─'), None);
        assert!(!is_blocks(""));
        assert!(!is_blocks("█a"));
        assert!(is_blocks("▐▛███▜▌"));
    }
}
