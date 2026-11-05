use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

use crate::icon::Icon;
use crate::id::{CommandId, TitlebarItemId, ViewId};
use crate::limits::MAX_TITLEBAR_ITEMS_PER_EXTENSION;
use crate::manifest::contributions::{Contributions, ViewZone};
use crate::manifest::ManifestError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TitlebarSide {
    Left,
    #[default]
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TitlebarAction {
    View(ViewId),
    Command(CommandId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "TitlebarItemFields", into = "TitlebarItemFields")]
pub struct TitlebarItemContribution {
    pub id: TitlebarItemId,
    pub title: String,
    pub icon: Icon,
    pub side: TitlebarSide,
    pub action: TitlebarAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("a titlebar item names exactly one of view or command")]
pub struct TitlebarActionError;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TitlebarItemFields {
    id: TitlebarItemId,
    title: String,
    icon: Icon,
    #[serde(default)]
    side: TitlebarSide,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    view: Option<ViewId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    command: Option<CommandId>,
}

impl TryFrom<TitlebarItemFields> for TitlebarItemContribution {
    type Error = TitlebarActionError;

    fn try_from(fields: TitlebarItemFields) -> Result<Self, Self::Error> {
        let action = match (fields.view, fields.command) {
            (Some(view), None) => TitlebarAction::View(view),
            (None, Some(command)) => TitlebarAction::Command(command),
            _ => return Err(TitlebarActionError),
        };
        Ok(Self {
            id: fields.id,
            title: fields.title,
            icon: fields.icon,
            side: fields.side,
            action,
        })
    }
}

impl From<TitlebarItemContribution> for TitlebarItemFields {
    fn from(item: TitlebarItemContribution) -> Self {
        let (view, command) = match item.action {
            TitlebarAction::View(view) => (Some(view), None),
            TitlebarAction::Command(command) => (None, Some(command)),
        };
        Self {
            id: item.id,
            title: item.title,
            icon: item.icon,
            side: item.side,
            view,
            command,
        }
    }
}

pub(super) fn check_titlebar_items(
    contributes: &Contributions,
    commands: &BTreeSet<&CommandId>,
) -> Result<(), ManifestError> {
    let items = &contributes.titlebar_items;
    if items.len() > MAX_TITLEBAR_ITEMS_PER_EXTENSION {
        return Err(ManifestError::TooManyTitlebarItems(items.len()));
    }
    super::unique(items.iter().map(|item| &item.id))?;
    super::unique(items.iter().filter_map(|item| match &item.action {
        TitlebarAction::View(view) => Some(view),
        TitlebarAction::Command(_) => None,
    }))?;
    for item in items {
        match &item.action {
            TitlebarAction::Command(command) if !commands.contains(command) => {
                return Err(ManifestError::UnknownCommand(command.clone()));
            }
            TitlebarAction::Command(_) => {}
            TitlebarAction::View(view) => {
                let zone = contributes
                    .views
                    .iter()
                    .find(|declared| declared.id == *view)
                    .map(|declared| declared.zone)
                    .ok_or_else(|| ManifestError::UnknownView(view.clone()))?;
                if zone != ViewZone::Popover {
                    return Err(ManifestError::NotAPopover(view.clone()));
                }
            }
        }
    }
    let unanchored = contributes
        .views
        .iter()
        .filter(|declared| declared.zone == ViewZone::Popover)
        .find(|declared| {
            !items
                .iter()
                .any(|item| item.action == TitlebarAction::View(declared.id.clone()))
        });
    match unanchored {
        Some(declared) => Err(ManifestError::UnanchoredPopover(declared.id.clone())),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ExtensionManifest;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const PLAYER: &str = r#"
id = "player"
name = "Player"
version = "0.1.0"
api = "1.1"

[program]
placement = "app"
runtime = "node"
args = ["dist/main.js"]

[[contributes.command]]
id = "player.next"
title = "Player: Next track"

[[contributes.view]]
id = "now-playing"
title = "Now playing"
zone = "popover"

[[contributes.titlebar_item]]
id = "now-playing"
title = "Now playing"
icon = "play"
side = "left"
view = "now-playing"

[[contributes.titlebar_item]]
id = "next"
title = "Next track"
icon = "chevron-right"
command = "player.next"
"#;

    fn player() -> Result<ExtensionManifest, toml::de::Error> {
        toml::from_str(PLAYER)
    }

    #[test]
    fn titlebar_items_read_with_their_side_and_action() -> TestResult {
        let manifest = player()?;
        let items = &manifest.contributes.titlebar_items;
        assert_eq!(items[0].side, TitlebarSide::Left);
        assert_eq!(items[0].icon, Icon::Play);
        assert_eq!(
            items[0].action,
            TitlebarAction::View("now-playing".parse()?)
        );
        assert_eq!(items[1].side, TitlebarSide::Right);
        assert_eq!(
            items[1].action,
            TitlebarAction::Command("player.next".parse()?)
        );
        assert_eq!(manifest.contributes.views[0].zone, ViewZone::Popover);
        assert_eq!(manifest.check(), Ok(()));
        Ok(())
    }

    #[test]
    fn an_item_round_trips_through_json() -> TestResult {
        let manifest = player()?;
        for item in &manifest.contributes.titlebar_items {
            let json = serde_json::to_value(item)?;
            assert!(json.get("action").is_none());
            let read: TitlebarItemContribution = serde_json::from_value(json)?;
            assert_eq!(&read, item);
        }
        Ok(())
    }

    #[test]
    fn an_item_needs_exactly_one_action_and_no_unknown_fields() {
        for bad in [
            "view = \"now-playing\"\ncommand = \"player.next\"",
            "",
            "view = \"now-playing\"\ntooltip = \"x\"",
        ] {
            let text = PLAYER.replace(
                "view = \"now-playing\"\n\n[[contributes.titlebar_item]]",
                &format!("{bad}\n\n[[contributes.titlebar_item]]"),
            );
            assert!(toml::from_str::<ExtensionManifest>(&text).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_item_must_open_a_declared_popover() -> TestResult {
        let mut manifest = player()?;
        manifest.contributes.titlebar_items[0].action = TitlebarAction::View("missing".parse()?);
        assert_eq!(
            manifest.check(),
            Err(ManifestError::UnknownView("missing".parse()?))
        );
        let mut manifest = player()?;
        manifest.contributes.views[0].zone = ViewZone::Sidebar;
        assert_eq!(
            manifest.check(),
            Err(ManifestError::NotAPopover("now-playing".parse()?))
        );
        Ok(())
    }

    #[test]
    fn a_popover_no_item_opens_fails_the_check() -> TestResult {
        let mut manifest = player()?;
        manifest.contributes.titlebar_items.remove(0);
        assert_eq!(
            manifest.check(),
            Err(ManifestError::UnanchoredPopover("now-playing".parse()?))
        );
        Ok(())
    }

    #[test]
    fn items_are_unique_bounded_and_run_declared_commands() -> TestResult {
        let mut manifest = player()?;
        let next = manifest.contributes.titlebar_items[1].clone();
        manifest.contributes.titlebar_items.push(next.clone());
        assert_eq!(
            manifest.check(),
            Err(ManifestError::Duplicate("next".to_owned()))
        );
        let mut manifest = player()?;
        let mut twin = manifest.contributes.titlebar_items[0].clone();
        twin.id = "twin".parse()?;
        manifest.contributes.titlebar_items.push(twin);
        assert_eq!(
            manifest.check(),
            Err(ManifestError::Duplicate("now-playing".to_owned()))
        );
        let mut manifest = player()?;
        for index in 0..MAX_TITLEBAR_ITEMS_PER_EXTENSION {
            let mut extra = next.clone();
            extra.id = format!("extra-{index}").parse()?;
            manifest.contributes.titlebar_items.push(extra);
        }
        assert_eq!(
            manifest.check(),
            Err(ManifestError::TooManyTitlebarItems(
                MAX_TITLEBAR_ITEMS_PER_EXTENSION + 2
            ))
        );
        let mut manifest = player()?;
        manifest.contributes.titlebar_items[1].action = TitlebarAction::Command("missing".parse()?);
        assert_eq!(
            manifest.check(),
            Err(ManifestError::UnknownCommand("missing".parse()?))
        );
        Ok(())
    }
}
