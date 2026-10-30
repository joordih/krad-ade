use crate::agent::AgentPreset;
use crate::gateway::GatewaySettings;
use crate::gitlab::GitlabSettings;
use crate::hue::Hue;
use crate::mcp::McpSettings;
use crate::model::{lenient_model_lists, ModelId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum AppearanceMode {
    #[default]
    Light,
    Dark,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum TerminalTheme {
    #[default]
    MatchApp,
    XcodeLight,
    XcodeDark,
    SolarizedLight,
    SolarizedDark,
    GitHubLight,
    OneDark,
    Dracula,
    Nord,
    TokyoNight,
    GruvboxDark,
}

impl TerminalTheme {
    #[must_use]
    pub const fn all() -> [Self; 11] {
        [
            Self::MatchApp,
            Self::XcodeLight,
            Self::XcodeDark,
            Self::SolarizedLight,
            Self::SolarizedDark,
            Self::GitHubLight,
            Self::OneDark,
            Self::Dracula,
            Self::Nord,
            Self::TokyoNight,
            Self::GruvboxDark,
        ]
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::MatchApp => "Match app",
            Self::XcodeLight => "Xcode Light",
            Self::XcodeDark => "Xcode Dark",
            Self::SolarizedLight => "Solarized Light",
            Self::SolarizedDark => "Solarized Dark",
            Self::GitHubLight => "GitHub Light",
            Self::OneDark => "One Dark",
            Self::Dracula => "Dracula",
            Self::Nord => "Nord",
            Self::TokyoNight => "Tokyo Night",
            Self::GruvboxDark => "Gruvbox Dark",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum TerminalFont {
    #[default]
    System,
    JetBrainsMono,
}

impl TerminalFont {
    #[must_use]
    pub const fn all() -> [Self; 2] {
        [Self::System, Self::JetBrainsMono]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Ligatures {
    #[default]
    On,
    Off,
}

impl Ligatures {
    #[must_use]
    pub const fn all() -> [Self; 2] {
        [Self::On, Self::Off]
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::On => "On",
            Self::Off => "Off",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum CopyMode {
    #[default]
    Shortcut,
    OnSelect,
    Both,
}

impl CopyMode {
    #[must_use]
    pub const fn all() -> [Self; 3] {
        [Self::Shortcut, Self::OnSelect, Self::Both]
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Shortcut => "Shortcut",
            Self::OnSelect => "On select",
            Self::Both => "Both",
        }
    }

    #[must_use]
    pub const fn copies_on_shortcut(self) -> bool {
        matches!(self, Self::Shortcut | Self::Both)
    }

    #[must_use]
    pub const fn copies_on_select(self) -> bool {
        matches!(self, Self::OnSelect | Self::Both)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub appearance: AppearanceMode,
    #[serde(default)]
    pub terminal_theme: TerminalTheme,
    #[serde(default)]
    pub default_harness: AgentPreset,
    #[serde(default)]
    pub harness_commands: HashMap<AgentPreset, String>,
    #[serde(default)]
    pub copy_mode: CopyMode,
    #[serde(default)]
    pub terminal_font: TerminalFont,
    #[serde(default)]
    pub ligatures: Ligatures,
    #[serde(
        default,
        deserialize_with = "lenient_model_lists",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub harness_models: HashMap<AgentPreset, Vec<ModelId>>,
    #[serde(default, skip_serializing_if = "GitlabSettings::is_empty")]
    pub gitlab: GitlabSettings,
    #[serde(default, skip_serializing_if = "GatewaySettings::is_empty")]
    pub gateway: GatewaySettings,
    #[serde(default, skip_serializing_if = "McpSettings::is_empty")]
    pub mcp: McpSettings,
    #[serde(default)]
    pub window_material: WindowMaterial,
    #[serde(default, skip_serializing_if = "WindowTint::is_default")]
    pub window_tint: WindowTint,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum WindowMaterial {
    #[default]
    Opaque,
    Blurred,
    Mica,
    Glass,
}

impl WindowMaterial {
    pub const ALL: [Self; 4] = [Self::Opaque, Self::Blurred, Self::Mica, Self::Glass];

    #[must_use]
    pub const fn is_translucent(self) -> bool {
        !matches!(self, Self::Opaque)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(from = "u8", into = "u8")]
pub struct TintOpacity(u8);

impl TintOpacity {
    pub const MAX: u8 = 100;

    #[must_use]
    pub const fn new(percent: u8) -> Self {
        if percent > Self::MAX {
            Self(Self::MAX)
        } else {
            Self(percent)
        }
    }

    #[must_use]
    pub const fn percent(self) -> u8 {
        self.0
    }
}

impl From<u8> for TintOpacity {
    fn from(percent: u8) -> Self {
        Self::new(percent)
    }
}

impl From<TintOpacity> for u8 {
    fn from(opacity: TintOpacity) -> Self {
        opacity.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct WindowTint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue: Option<Hue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<TintOpacity>,
}

impl WindowTint {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Settings {
    #[must_use]
    pub fn models(&self, preset: AgentPreset) -> Vec<ModelId> {
        self.harness_models
            .get(&preset)
            .cloned()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn launch_line(&self, preset: AgentPreset) -> Option<String> {
        let program = preset.program()?;
        let custom = self
            .harness_commands
            .get(&preset)
            .map(|line| line.trim())
            .filter(|line| !line.is_empty());
        Some(custom.unwrap_or(program).to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_harness_command_replaces_the_bare_program() {
        let mut settings = Settings::default();
        assert_eq!(settings.launch_line(AgentPreset::Shell), None);
        assert_eq!(
            settings.launch_line(AgentPreset::ClaudeCode).as_deref(),
            Some("claude")
        );
        settings
            .harness_commands
            .insert(AgentPreset::ClaudeCode, "  claude --model opus ".to_owned());
        settings
            .harness_commands
            .insert(AgentPreset::Codex, "   ".to_owned());
        assert_eq!(
            settings.launch_line(AgentPreset::ClaudeCode).as_deref(),
            Some("claude --model opus")
        );
        assert_eq!(
            settings.launch_line(AgentPreset::Codex).as_deref(),
            Some("codex"),
            "a blank override falls back to the program"
        );
    }

    #[test]
    fn settings_roundtrip_and_old_files_get_defaults() -> serde_json::Result<()> {
        let mut settings = Settings {
            terminal_theme: TerminalTheme::Dracula,
            default_harness: AgentPreset::Codex,
            copy_mode: CopyMode::Both,
            ..Settings::default()
        };
        settings
            .harness_commands
            .insert(AgentPreset::ClaudeCode, "claude --model opus".to_owned());
        let back: Settings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back, settings);
        let partial: Settings =
            serde_json::from_str(r#"{"terminal_theme":"Nord","agent_surface":"Chat"}"#)?;
        assert_eq!(partial.terminal_theme, TerminalTheme::Nord);
        assert_eq!(partial.default_harness, AgentPreset::Shell);
        assert_eq!(partial.copy_mode, CopyMode::Shortcut);
        assert_eq!(
            partial.ligatures,
            Ligatures::On,
            "fonts join ligatures unless turned off"
        );
        Ok(())
    }

    #[test]
    fn stored_lists_drop_invalid_and_repeated_names() -> serde_json::Result<()> {
        let stored: Settings = serde_json::from_str(
            r#"{"terminal_theme":"Nord",
            "harness_models":{"ClaudeCode":["opus"," ","opus","two words","haiku"]},
            "gateway":{"models":["gemini-3-pro","a&b","gemini-3-pro"]}}"#,
        )?;
        assert_eq!(stored.terminal_theme, TerminalTheme::Nord);
        let names = |models: &[ModelId]| -> Vec<String> {
            models.iter().map(ToString::to_string).collect()
        };
        assert_eq!(
            names(&stored.models(AgentPreset::ClaudeCode)),
            ["opus", "haiku"]
        );
        assert_eq!(names(&stored.gateway.models), ["gemini-3-pro"]);
        Ok(())
    }

    #[test]
    fn harnesses_offer_no_models_until_some_are_added() -> serde_json::Result<()> {
        let old: Settings = serde_json::from_str(
            r#"{"appearance":"Dark","default_harness":"Codex","harness_commands":{"Codex":"codex"}}"#,
        )?;
        assert!(old.harness_models.is_empty());
        assert_eq!(old.models(AgentPreset::Codex), Vec::<ModelId>::new());
        let unchanged = serde_json::to_string(&Settings::default())?;
        assert!(
            !unchanged.contains("harness_models"),
            "untouched lists stay out of the file"
        );
        Ok(())
    }

    #[test]
    fn a_window_tint_is_capped_and_left_out_until_picked() -> serde_json::Result<()> {
        let plain = serde_json::to_string(&Settings::default())?;
        assert!(!plain.contains("window_tint"));
        let read: Settings =
            serde_json::from_str(r#"{"window_tint":{"hue":"blue","opacity":140}}"#)?;
        assert_eq!(
            read.window_tint,
            WindowTint {
                hue: Some(Hue::Blue),
                opacity: Some(TintOpacity::new(TintOpacity::MAX)),
            }
        );
        Ok(())
    }
}
