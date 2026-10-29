use crate::hue::Hue;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionIcon {
    Folder,
    Terminal,
    Book,
    Note,
    Flag,
    Branch,
    Layers,
    Home,
    Spark,
    Chat,
    Tasks,
    Diamond,
    Plug,
    People,
    Flow,
    Target,
}

impl SessionIcon {
    pub const ALL: [Self; 16] = [
        Self::Folder,
        Self::Terminal,
        Self::Book,
        Self::Note,
        Self::Flag,
        Self::Branch,
        Self::Layers,
        Self::Home,
        Self::Spark,
        Self::Chat,
        Self::Tasks,
        Self::Diamond,
        Self::Plug,
        Self::People,
        Self::Flow,
        Self::Target,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Hue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<SessionIcon>,
}

impl SessionLook {
    #[must_use]
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_look_is_left_out_and_a_picked_one_round_trips() -> serde_json::Result<()> {
        assert!(SessionLook::default().is_plain());
        let look = SessionLook {
            color: Some(Hue::Teal),
            icon: Some(SessionIcon::Spark),
        };
        let json = serde_json::to_string(&look)?;
        assert_eq!(json, r#"{"color":"teal","icon":"spark"}"#);
        assert_eq!(serde_json::from_str::<SessionLook>(&json)?, look);
        Ok(())
    }
}
