mod color;

use serde::{Deserialize, Serialize};

pub use color::{ThemeColor, ThemeColorError};

pub const ANSI_COLORS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThemeId(pub String);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeAppearance {
    Light,
    #[default]
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeCredit {
    pub author: String,
    pub license: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterfaceColors {
    pub background: ThemeColor,
    pub surface: ThemeColor,
    pub panel: ThemeColor,
    pub text: ThemeColor,
    pub muted_text: ThemeColor,
    pub accent: ThemeColor,
    pub on_accent: ThemeColor,
    pub success: ThemeColor,
    pub warning: ThemeColor,
    pub error: ThemeColor,
    pub info: ThemeColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalColors {
    pub background: ThemeColor,
    pub foreground: ThemeColor,
    pub cursor: ThemeColor,
    pub selection: ThemeColor,
    pub ansi: [ThemeColor; ANSI_COLORS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxColors {
    pub comment: ThemeColor,
    pub keyword: ThemeColor,
    pub string: ThemeColor,
    pub escape: ThemeColor,
    pub number: ThemeColor,
    pub constant: ThemeColor,
    pub attribute: ThemeColor,
    #[serde(rename = "type")]
    pub type_name: ThemeColor,
    pub operator: ThemeColor,
    pub function: ThemeColor,
    pub property: ThemeColor,
    pub tag: ThemeColor,
    pub variable: ThemeColor,
    pub punctuation: ThemeColor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub id: ThemeId,
    pub name: String,
    pub appearance: ThemeAppearance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit: Option<ThemeCredit>,
    pub interface: InterfaceColors,
    pub terminal: TerminalColors,
    pub syntax: SyntaxColors,
}
