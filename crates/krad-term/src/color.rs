use libghostty_vt::style::{Palette, RgbColor};

pub const ANSI_COLORS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnsiPalette(pub [u32; ANSI_COLORS]);

pub const XTERM_PALETTE: AnsiPalette = AnsiPalette([
    0x00_00_00, 0xcd_00_00, 0x00_cd_00, 0xcd_cd_00, 0x00_00_ee, 0xcd_00_cd, 0x00_cd_cd, 0xe5_e5_e5,
    0x7f_7f_7f, 0xff_00_00, 0x00_ff_00, 0xff_ff_00, 0x5c_5c_ff, 0xff_00_ff, 0x00_ff_ff, 0xff_ff_ff,
]);

const PALETTE_SIZE: usize = 256;
const CUBE_LEVELS: [u8; CUBE_SIDE] = [0, 95, 135, 175, 215, 255];
const CUBE_SIDE: usize = 6;
const GRAY_START: usize = 232;
const GRAY_BASE: u8 = 8;
const GRAY_STEP: u8 = 10;
const RED_SHIFT: u32 = 16;
const GREEN_SHIFT: u32 = 8;
const CHANNEL_MASK: u32 = 0xff;
const BLACK: RgbColor = RgbColor { r: 0, g: 0, b: 0 };

pub(crate) fn pack(color: RgbColor) -> u32 {
    (u32::from(color.r) << RED_SHIFT) | (u32::from(color.g) << GREEN_SHIFT) | u32::from(color.b)
}

fn unpack(color: u32) -> RgbColor {
    let channel = |shift: u32| u8::try_from((color >> shift) & CHANNEL_MASK).unwrap_or(u8::MAX);
    RgbColor {
        r: channel(RED_SHIFT),
        g: channel(GREEN_SHIFT),
        b: channel(0),
    }
}

fn indexed(index: usize, ansi: &AnsiPalette) -> RgbColor {
    if let Some(color) = ansi.0.get(index) {
        return unpack(*color);
    }
    if index < GRAY_START {
        let step = index - ANSI_COLORS;
        return RgbColor {
            r: CUBE_LEVELS[step / (CUBE_SIDE * CUBE_SIDE)],
            g: CUBE_LEVELS[step / CUBE_SIDE % CUBE_SIDE],
            b: CUBE_LEVELS[step % CUBE_SIDE],
        };
    }
    let ramp = u8::try_from(index - GRAY_START).unwrap_or(u8::MAX);
    let gray = GRAY_BASE + GRAY_STEP * ramp;
    RgbColor {
        r: gray,
        g: gray,
        b: gray,
    }
}

pub(crate) fn full_palette(ansi: &AnsiPalette) -> Palette {
    let mut colors = [BLACK; PALETTE_SIZE];
    for (index, color) in colors.iter_mut().enumerate() {
        *color = indexed(index, ansi);
    }
    Palette(colors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_keeps_the_theme_colors_then_the_xterm_cube_and_grays() {
        let Palette(colors) = full_palette(&XTERM_PALETTE);
        assert_eq!(pack(colors[1]), XTERM_PALETTE.0[1]);
        assert_eq!(pack(colors[196]), 0xff_00_00);
        assert_eq!(pack(colors[22]), 0x00_5f_00);
        assert_eq!(pack(colors[244]), 0x80_80_80);
    }
}
