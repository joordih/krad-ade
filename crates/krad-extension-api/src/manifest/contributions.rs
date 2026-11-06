use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::icon::Icon;
use crate::id::{CommandId, HostKindId, SecretName, SettingKey, ToolName, ViewId};
use crate::manifest::titlebar::TitlebarItemContribution;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Contributions {
    #[serde(default, rename = "command")]
    pub commands: Vec<CommandContribution>,
    #[serde(default, rename = "menu")]
    pub menus: Vec<MenuContribution>,
    #[serde(default, rename = "keybinding")]
    pub keybindings: Vec<KeybindingContribution>,
    #[serde(default, rename = "view")]
    pub views: Vec<ViewContribution>,
    #[serde(default, rename = "setting")]
    pub settings: Vec<SettingContribution>,
    #[serde(default, rename = "secret")]
    pub secrets: Vec<SecretContribution>,
    #[serde(default, rename = "tool")]
    pub tools: Vec<ToolContribution>,
    #[serde(default, rename = "host_kind")]
    pub host_kinds: Vec<HostKindContribution>,
    #[serde(default, rename = "titlebar_item")]
    pub titlebar_items: Vec<TitlebarItemContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandContribution {
    pub id: CommandId,
    pub title: String,
    #[serde(default)]
    pub icon: Option<Icon>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MenuTarget {
    File,
    Folder,
    Tab,
    Group,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MenuContribution {
    pub command: CommandId,
    pub target: MenuTarget,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeybindingContribution {
    pub command: CommandId,
    #[serde(default)]
    pub macos: Option<String>,
    #[serde(default)]
    pub linux: Option<String>,
    #[serde(default)]
    pub windows: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewZone {
    Sidebar,
    SidePanel,
    Utilities,
    FileTab,
    Popover,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewContribution {
    pub id: ViewId,
    pub title: String,
    pub zone: ViewZone,
    #[serde(default)]
    pub icon: Option<Icon>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SettingContribution {
    pub key: SettingKey,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(flatten)]
    pub kind: SettingKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SettingKind {
    Text {
        #[serde(default)]
        default: String,
    },
    Toggle {
        #[serde(default)]
        default: bool,
    },
    Number {
        default: f64,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    Choice {
        options: Vec<SettingOption>,
        default: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecretContribution {
    pub name: SecretName,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    #[default]
    ReadOnly,
    Destructive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolContribution {
    pub name: ToolName,
    pub description: String,
    #[serde(default)]
    pub effect: ToolEffect,
    #[serde(default = "empty_object_schema")]
    pub input_schema: serde_json::Value,
}

fn empty_object_schema() -> serde_json::Value {
    serde_json::json!({ "type": "object" })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostKindContribution {
    pub id: HostKindId,
    pub title: String,
    #[serde(default)]
    pub icon: Option<Icon>,
}
