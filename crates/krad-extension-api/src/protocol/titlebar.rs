use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::icon::Icon;
use crate::id::TitlebarItemId;
use crate::limits::MAX_TITLEBAR_TITLE_CHARS;
use crate::protocol::notification;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SetTitlebarItemParams {
    pub item: TitlebarItemId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
}

impl SetTitlebarItemParams {
    #[must_use]
    pub fn bounded(self) -> Self {
        Self {
            title: self
                .title
                .map(|title| title.chars().take(MAX_TITLEBAR_TITLE_CHARS).collect()),
            ..self
        }
    }
}

notification!(SetTitlebarItem, "titlebar/set", App, SetTitlebarItemParams);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Notification, Party};
    use serde_json::json;

    #[test]
    fn a_state_change_reads_only_the_fields_it_sends() -> Result<(), Box<dyn std::error::Error>> {
        let read: SetTitlebarItemParams =
            serde_json::from_value(json!({"item": "now-playing", "active": true}))?;
        assert_eq!(
            read,
            SetTitlebarItemParams {
                item: "now-playing".parse()?,
                icon: None,
                title: None,
                active: Some(true),
                hidden: None,
            }
        );
        assert_eq!(
            serde_json::to_value(&read)?,
            json!({"item": "now-playing", "active": true})
        );
        let full = SetTitlebarItemParams {
            icon: Some(Icon::Stop),
            title: Some("Pause".to_owned()),
            hidden: Some(false),
            ..read
        };
        let back: SetTitlebarItemParams = serde_json::from_value(serde_json::to_value(&full)?)?;
        assert_eq!(back, full);
        assert_eq!(SetTitlebarItem::METHOD, "titlebar/set");
        assert_eq!(SetTitlebarItem::RECEIVED_BY, Party::App);
        Ok(())
    }

    #[test]
    fn a_long_tooltip_is_cut_on_receive() -> Result<(), crate::IdError> {
        let params = SetTitlebarItemParams {
            item: "now-playing".parse()?,
            icon: None,
            title: Some("é".repeat(MAX_TITLEBAR_TITLE_CHARS * 2)),
            active: None,
            hidden: None,
        };
        let title = params.bounded().title.unwrap_or_default();
        assert_eq!(title.chars().count(), MAX_TITLEBAR_TITLE_CHARS);
        Ok(())
    }
}
