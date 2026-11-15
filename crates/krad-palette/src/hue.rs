use krad_domain::Hue;

const BLUE: u32 = 0x0040_8cff;
const PURPLE: u32 = 0x009b_6bf2;
const PINK: u32 = 0x00e8_5d9c;
const RED: u32 = 0x00e5_4d4d;
const ORANGE: u32 = 0x00f0_8a3c;
const YELLOW: u32 = 0x00e6_b829;
const GREEN: u32 = 0x003f_b96b;
const TEAL: u32 = 0x0026_b0b0;

#[must_use]
pub const fn hue_rgb(hue: Hue, gray: u32) -> u32 {
    match hue {
        Hue::Blue => BLUE,
        Hue::Purple => PURPLE,
        Hue::Pink => PINK,
        Hue::Red => RED,
        Hue::Orange => ORANGE,
        Hue::Yellow => YELLOW,
        Hue::Green => GREEN,
        Hue::Teal => TEAL,
        Hue::Gray => gray,
    }
}
