use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hue {
    Blue,
    Purple,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Gray,
}

impl Hue {
    pub const ALL: [Self; 9] = [
        Self::Blue,
        Self::Purple,
        Self::Pink,
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Teal,
        Self::Gray,
    ];
}
