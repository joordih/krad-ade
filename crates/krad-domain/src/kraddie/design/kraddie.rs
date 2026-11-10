use super::grid::{CanvasPoint, PaletteIndex, PixelGrid};
use super::paint::{filled_ellipse, line, rectangle};
use super::{KraddieAnimation, KraddieColor, KraddieDesign, KraddieLayer, MoodLook, MoodLooks};
use crate::kraddie::mood::KraddieMood;
use serde::{Deserialize, Serialize};

const OUTLINE: PaletteIndex = PaletteIndex::wrapping(0);
const BODY: PaletteIndex = PaletteIndex::wrapping(1);
const SHADE: PaletteIndex = PaletteIndex::wrapping(2);
const WHITE: PaletteIndex = PaletteIndex::wrapping(3);
const CHEEK: PaletteIndex = PaletteIndex::wrapping(4);
const ACCENT: PaletteIndex = PaletteIndex::wrapping(5);
const ALARM: PaletteIndex = PaletteIndex::wrapping(6);
const BODY_LAYER: &str = "Body";
const DETAIL_LAYER: &str = "Details";
const SHARED_COLORS: [u32; 5] = [
    0x00f4_f4f8,
    0x00ff_8fab,
    0x00ff_d166,
    0x00ef_476f,
    0x0006_d6a0,
];
const OUTLINE_COLOR: u32 = 0x001d_1b2e;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyStyle {
    #[default]
    Kraken,
    Blob,
    Bot,
    Ghost,
}

#[derive(Clone, Copy)]
struct Face {
    left_eye: i32,
    right_eye: i32,
    eye_y: i32,
    mouth_y: i32,
}

impl BodyStyle {
    pub const ALL: [Self; 4] = [Self::Kraken, Self::Blob, Self::Bot, Self::Ghost];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Kraken => "Kraken",
            Self::Blob => "Blob",
            Self::Bot => "Bot",
            Self::Ghost => "Ghost",
        }
    }

    const fn colors(self) -> (u32, u32) {
        match self {
            Self::Kraken => (0x0097_6bff, 0x006a_44d6),
            Self::Blob => (0x0055_d6a8, 0x0031_a37c),
            Self::Bot => (0x0079_b8ff, 0x004d_86d1),
            Self::Ghost => (0x00e6_e1f5, 0x00b8_b0d4),
        }
    }

    const fn face(self) -> Face {
        match self {
            Self::Kraken => Face {
                left_eye: 9,
                right_eye: 14,
                eye_y: 8,
                mouth_y: 12,
            },
            Self::Blob => Face {
                left_eye: 8,
                right_eye: 15,
                eye_y: 13,
                mouth_y: 17,
            },
            Self::Bot => Face {
                left_eye: 8,
                right_eye: 15,
                eye_y: 10,
                mouth_y: 15,
            },
            Self::Ghost => Face {
                left_eye: 9,
                right_eye: 14,
                eye_y: 7,
                mouth_y: 11,
            },
        }
    }
}

fn at(x: i32, y: i32) -> CanvasPoint {
    CanvasPoint::clamped(x, y)
}

fn dot(grid: &mut PixelGrid, x: i32, y: i32, color: PaletteIndex) {
    grid.set_signed(x, y, Some(color));
}

fn body(style: BodyStyle) -> (PixelGrid, PixelGrid) {
    let mut fill = PixelGrid::default();
    let mut details = PixelGrid::default();
    let ink = Some;
    match style {
        BodyStyle::Kraken => {
            filled_ellipse(&mut fill, at(4, 1), at(19, 16), ink(OUTLINE));
            filled_ellipse(&mut fill, at(5, 2), at(18, 15), ink(BODY));
            for x in [5, 9, 14, 18] {
                line(&mut details, at(x, 15), at(x, 20), ink(OUTLINE));
                line(&mut details, at(x + 1, 15), at(x + 1, 19), ink(SHADE));
                dot(&mut details, x + 1, 21, OUTLINE);
            }
            line(&mut details, at(7, 13), at(16, 13), ink(SHADE));
        }
        BodyStyle::Blob => {
            filled_ellipse(&mut fill, at(2, 7), at(21, 22), ink(OUTLINE));
            filled_ellipse(&mut fill, at(3, 8), at(20, 21), ink(BODY));
            line(&mut details, at(6, 20), at(17, 20), ink(SHADE));
            dot(&mut details, 6, 10, WHITE);
            dot(&mut details, 7, 10, WHITE);
        }
        BodyStyle::Bot => {
            rectangle(&mut fill, at(4, 4), at(19, 19), ink(OUTLINE));
            for y in 5..19 {
                line(&mut fill, at(5, y), at(18, y), ink(BODY));
            }
            line(&mut details, at(11, 1), at(11, 3), ink(OUTLINE));
            dot(&mut details, 11, 0, ACCENT);
            line(&mut details, at(5, 18), at(18, 18), ink(SHADE));
            rectangle(&mut details, at(8, 20), at(15, 21), ink(OUTLINE));
        }
        BodyStyle::Ghost => {
            filled_ellipse(&mut fill, at(4, 1), at(19, 14), ink(OUTLINE));
            for y in 8..20 {
                line(&mut fill, at(4, y), at(19, y), ink(OUTLINE));
            }
            filled_ellipse(&mut fill, at(5, 2), at(18, 13), ink(BODY));
            for y in 8..20 {
                line(&mut fill, at(5, y), at(18, y), ink(BODY));
            }
            for x in (5..19).step_by(4) {
                dot(&mut details, x, 20, OUTLINE);
                dot(&mut details, x + 1, 20, OUTLINE);
                dot(&mut details, x + 2, 19, SHADE);
            }
        }
    }
    (fill, details)
}

fn eyes(grid: &mut PixelGrid, face: Face, mood: KraddieMood) {
    for x in [face.left_eye, face.right_eye] {
        let y = face.eye_y;
        match mood {
            KraddieMood::Idle | KraddieMood::Waiting => {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    dot(grid, x + dx, y + dy, WHITE);
                }
                let look_up = i32::from(mood == KraddieMood::Waiting);
                dot(grid, x + 1, y + 1 - look_up, OUTLINE);
            }
            KraddieMood::Working => {
                line(grid, at(x - 1, y + 1), at(x + 1, y + 1), Some(OUTLINE));
                dot(grid, x, y + 2, WHITE);
            }
            KraddieMood::Done => {
                dot(grid, x - 1, y + 1, OUTLINE);
                dot(grid, x, y, OUTLINE);
                dot(grid, x + 1, y + 1, OUTLINE);
            }
            KraddieMood::Error => {
                line(grid, at(x - 1, y - 1), at(x + 1, y + 1), Some(ALARM));
                line(grid, at(x - 1, y + 1), at(x + 1, y - 1), Some(ALARM));
            }
        }
    }
}

fn mouth(grid: &mut PixelGrid, face: Face, mood: KraddieMood) {
    let centre = (face.left_eye + face.right_eye + 1) / 2;
    let y = face.mouth_y;
    let ink = Some(OUTLINE);
    match mood {
        KraddieMood::Idle => {
            dot(grid, centre - 2, y - 1, OUTLINE);
            line(grid, at(centre - 1, y), at(centre, y), ink);
            dot(grid, centre + 1, y - 1, OUTLINE);
        }
        KraddieMood::Working => line(grid, at(centre - 1, y), at(centre, y), ink),
        KraddieMood::Waiting => rectangle(grid, at(centre - 1, y - 1), at(centre, y), ink),
        KraddieMood::Done => {
            dot(grid, centre - 3, y - 1, OUTLINE);
            line(grid, at(centre - 2, y), at(centre + 1, y), ink);
            dot(grid, centre + 2, y - 1, OUTLINE);
            dot(grid, face.left_eye - 2, y - 1, CHEEK);
            dot(grid, face.right_eye + 3, y - 1, CHEEK);
        }
        KraddieMood::Error => {
            dot(grid, centre - 2, y, OUTLINE);
            line(grid, at(centre - 1, y - 1), at(centre, y - 1), ink);
            dot(grid, centre + 1, y, OUTLINE);
        }
    }
}

const fn mood_animation(mood: KraddieMood) -> KraddieAnimation {
    match mood {
        KraddieMood::Idle => KraddieAnimation::Breathe,
        KraddieMood::Working => KraddieAnimation::Bounce,
        KraddieMood::Waiting => KraddieAnimation::Sway,
        KraddieMood::Done => KraddieAnimation::Hop,
        KraddieMood::Error => KraddieAnimation::Shake,
    }
}

fn look(style: BodyStyle, mood: KraddieMood) -> MoodLook {
    let mut expression = PixelGrid::default();
    eyes(&mut expression, style.face(), mood);
    mouth(&mut expression, style.face(), mood);
    MoodLook {
        expression,
        animation: mood_animation(mood),
    }
}

impl KraddieDesign {
    #[must_use]
    pub fn kraddie(style: BodyStyle) -> Self {
        let (body_color, shade_color) = style.colors();
        let palette = [OUTLINE_COLOR, body_color, shade_color]
            .into_iter()
            .chain(SHARED_COLORS)
            .map(KraddieColor::rgb)
            .collect();
        let (fill, details) = body(style);
        let layer = |name: &str, pixels| KraddieLayer {
            pixels,
            ..KraddieLayer::new(name)
        };
        Self {
            style,
            palette,
            layers: vec![layer(BODY_LAYER, fill), layer(DETAIL_LAYER, details)],
            moods: MoodLooks {
                idle: look(style, KraddieMood::Idle),
                working: look(style, KraddieMood::Working),
                waiting: look(style, KraddieMood::Waiting),
                done: look(style, KraddieMood::Done),
                error: look(style, KraddieMood::Error),
            },
        }
    }
}
