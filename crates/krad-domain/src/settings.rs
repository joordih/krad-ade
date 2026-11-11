use crate::acp::AcpSettings;
use crate::agent::AgentPreset;
use crate::attention::AttentionSettings;
use crate::backdrop_image::BackdropImage;
use crate::composer_height::ComposerHeight;
use crate::event::WebhookSettings;
use crate::extension_index::{lenient_index_repository, IndexRepository};
use crate::file_icon::FileIconColors;
use crate::font::{stored_font, FontChoice};
use crate::gateway::GatewaySettings;
use crate::gitlab::GitlabSettings;
use crate::host_timeouts::HostTimeouts;
use crate::hue::Hue;
use crate::ids::EnvironmentName;
use crate::java_engine::JavaEngine;
use crate::mcp::McpSettings;
use crate::model::{lenient_model_lists, ChatChoice, ModelId, ModelPreferences};
use crate::plan_usage::PlanUsageSettings;
use crate::problem::EditorSettings;
use crate::server_runtime::{ExtensionEntry, ServerRuntimePaths};
use crate::sidebar_layout::SidebarLayout;
use crate::skill::SkillSettings;
use crate::theme::{ThemeAppearance, ThemeId};
use crate::theme_tuning::{EnvironmentThemes, ThemeTuning};
use crate::toolchain::ToolchainActivation;
use crate::usage::SpendSettings;
use crate::worktree_settings::WorktreeSettings;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum AppearanceMode {
    #[default]
    Light,
    Dark,
    System,
}

pub const KRAD_LIGHT_THEME: &str = "krad-light";
pub const KRAD_DARK_THEME: &str = "krad-dark";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSelection {
    pub light: ThemeId,
    pub dark: ThemeId,
}

impl Default for ThemeSelection {
    fn default() -> Self {
        Self {
            light: ThemeId(KRAD_LIGHT_THEME.to_owned()),
            dark: ThemeId(KRAD_DARK_THEME.to_owned()),
        }
    }
}

impl ThemeSelection {
    #[must_use]
    pub const fn for_appearance(&self, appearance: ThemeAppearance) -> &ThemeId {
        match appearance {
            ThemeAppearance::Light => &self.light,
            ThemeAppearance::Dark => &self.dark,
        }
    }

    pub fn set(&mut self, appearance: ThemeAppearance, id: ThemeId) {
        match appearance {
            ThemeAppearance::Light => self.light = id,
            ThemeAppearance::Dark => self.dark = id,
        }
    }
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
    const fn library_theme(self) -> Option<(ThemeAppearance, &'static str)> {
        match self {
            Self::MatchApp => None,
            Self::XcodeLight => Some((ThemeAppearance::Light, "xcode-light")),
            Self::XcodeDark => Some((ThemeAppearance::Dark, "xcode-dark")),
            Self::SolarizedLight => Some((ThemeAppearance::Light, "solarized-light")),
            Self::SolarizedDark => Some((ThemeAppearance::Dark, "solarized-dark")),
            Self::GitHubLight => Some((ThemeAppearance::Light, "github-light-default")),
            Self::OneDark => Some((ThemeAppearance::Dark, "one-dark-pro")),
            Self::Dracula => Some((ThemeAppearance::Dark, "dracula")),
            Self::Nord => Some((ThemeAppearance::Dark, "nord")),
            Self::TokyoNight => Some((ThemeAppearance::Dark, "tokyo-night")),
            Self::GruvboxDark => Some((ThemeAppearance::Dark, "gruvbox-dark-medium")),
        }
    }

    #[must_use]
    pub fn as_selection(self) -> ThemeSelection {
        let mut selection = ThemeSelection::default();
        if let Some((appearance, id)) = self.library_theme() {
            selection.set(appearance, ThemeId(id.to_owned()));
        }
        selection
    }

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

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum HarnessOnboarding {
    #[default]
    Pending,
    Done,
}

impl HarnessOnboarding {
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum AppUpdateCheck {
    #[default]
    Automatic,
    Manual,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeTools {
    #[default]
    On,
    Off,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiDiscoveryFile {
    #[default]
    On,
    Off,
}

impl ApiDiscoveryFile {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionTools {
    #[default]
    On,
    Off,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PerformanceMode {
    #[default]
    Off,
    On,
}

impl PerformanceMode {
    pub const ALL: [Self; 2] = [Self::Off, Self::On];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::On => "On",
        }
    }

    #[must_use]
    pub const fn reduces_motion(self) -> bool {
        matches!(self, Self::On)
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub appearance: AppearanceMode,
    #[serde(default)]
    pub code_tools: CodeTools,
    #[serde(default)]
    pub extension_tools: ExtensionTools,
    #[serde(default, skip_serializing)]
    pub terminal_theme: TerminalTheme,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub themes: Option<ThemeSelection>,
    #[serde(default, skip_serializing_if = "EnvironmentThemes::is_empty")]
    pub environment_themes: EnvironmentThemes,
    #[serde(default, skip_serializing_if = "ThemeTuning::is_default")]
    pub theme_tuning: ThemeTuning,
    #[serde(default)]
    pub default_harness: AgentPreset,
    #[serde(default)]
    pub harness_commands: HashMap<AgentPreset, String>,
    #[serde(default)]
    pub copy_mode: CopyMode,
    #[serde(default, deserialize_with = "stored_font")]
    pub terminal_font: FontChoice,
    #[serde(default)]
    pub editor_font: FontChoice,
    #[serde(default)]
    pub interface_font: FontChoice,
    #[serde(
        default,
        deserialize_with = "lenient_model_lists",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub harness_models: HashMap<AgentPreset, Vec<ModelId>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub chat_defaults: HashMap<AgentPreset, ChatChoice>,
    #[serde(default, skip_serializing_if = "ModelPreferences::is_empty")]
    pub model_preferences: ModelPreferences,
    #[serde(default, skip_serializing_if = "GitlabSettings::is_empty")]
    pub gitlab: GitlabSettings,
    #[serde(default, skip_serializing_if = "GatewaySettings::is_empty")]
    pub gateway: GatewaySettings,
    #[serde(default, skip_serializing_if = "McpSettings::is_empty")]
    pub mcp: McpSettings,
    #[serde(default, skip_serializing_if = "AcpSettings::is_empty")]
    pub acp: AcpSettings,
    #[serde(default)]
    pub window_material: WindowMaterial,
    #[serde(default, skip_serializing_if = "WindowTint::is_default")]
    pub window_tint: WindowTint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop_image: Option<BackdropImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_backdrop_image: Option<BackdropImage>,
    #[serde(default)]
    pub app_updates: AppUpdateCheck,
    #[serde(
        default,
        alias = "toolchains",
        skip_serializing_if = "ServerRuntimePaths::is_empty"
    )]
    pub server_runtimes: ServerRuntimePaths,
    #[serde(
        default,
        deserialize_with = "crate::server_runtime::lenient_extensions",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub extensions: Vec<ExtensionEntry>,
    #[serde(
        default,
        deserialize_with = "lenient_index_repository",
        skip_serializing_if = "IndexRepository::is_default"
    )]
    pub extension_index: IndexRepository,
    #[serde(default, skip_serializing_if = "HostTimeouts::is_default")]
    pub host_timeouts: HostTimeouts,
    #[serde(default, skip_serializing_if = "ToolchainActivation::is_off")]
    pub toolchain_activation: ToolchainActivation,
    #[serde(default, skip_serializing_if = "EditorSettings::is_default")]
    pub editor: EditorSettings,
    #[serde(default, skip_serializing_if = "FileIconColors::is_default")]
    pub file_icon_colors: FileIconColors,
    #[serde(default, skip_serializing_if = "JavaEngine::is_default")]
    pub java_engine: JavaEngine,
    #[serde(default, skip_serializing_if = "PerformanceMode::is_default")]
    pub performance_mode: PerformanceMode,
    #[serde(default, skip_serializing_if = "SidebarLayout::is_default")]
    pub sidebar: SidebarLayout,
    #[serde(default, skip_serializing_if = "ComposerHeight::is_default")]
    pub composer_height: ComposerHeight,
    #[serde(
        default,
        deserialize_with = "crate::attention::lenient_attention",
        skip_serializing_if = "AttentionSettings::is_default"
    )]
    pub attention: AttentionSettings,
    #[serde(default, skip_serializing_if = "HarnessOnboarding::is_pending")]
    pub harness_onboarding: HarnessOnboarding,
    #[serde(default, skip_serializing_if = "SpendSettings::is_default")]
    pub spending: SpendSettings,
    #[serde(default, skip_serializing_if = "PlanUsageSettings::is_default")]
    pub plan_usage: PlanUsageSettings,
    #[serde(default, skip_serializing_if = "WebhookSettings::is_empty")]
    pub webhooks: WebhookSettings,
    #[serde(default, skip_serializing_if = "ApiDiscoveryFile::is_default")]
    pub api_discovery_file: ApiDiscoveryFile,
    #[serde(default, skip_serializing_if = "SkillSettings::is_empty")]
    pub skills: SkillSettings,
    #[serde(default, skip_serializing_if = "WorktreeSettings::is_default")]
    pub worktrees: WorktreeSettings,
}

impl Settings {
    pub fn migrate_themes(&mut self) -> &ThemeSelection {
        let legacy = self.terminal_theme;
        self.themes.get_or_insert_with(|| legacy.as_selection())
    }

    #[must_use]
    pub fn theme_selection(&self, environment: Option<&EnvironmentName>) -> ThemeSelection {
        let app = self.themes.clone().unwrap_or_default();
        self.environment_themes.selection(environment, &app)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum WindowMaterial {
    Opaque,
    Blurred,
    Mica,
    #[default]
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
        let program = match preset.acp_agent() {
            Some(id) => self.acp.agent(id)?.terminal?,
            None => preset.program()?.to_owned(),
        };
        let custom = self
            .harness_commands
            .get(&preset)
            .map(|line| line.trim())
            .filter(|line| !line.is_empty());
        Some(custom.map_or(program, str::to_owned))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_backdrop_image_survives_a_restart_and_no_image_stays_out_of_the_file(
    ) -> serde_json::Result<()> {
        let plain = serde_json::to_string(&Settings::default())?;
        assert!(!plain.contains("backdrop_image"));
        let settings = Settings {
            backdrop_image: Some(BackdropImage::new("backdrop/image.png")),
            ..Settings::default()
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back.backdrop_image, settings.backdrop_image);
        Ok(())
    }

    #[test]
    fn performance_mode_is_off_by_default_and_stays_out_of_the_file() -> serde_json::Result<()> {
        let settings = Settings::default();
        assert_eq!(settings.performance_mode, PerformanceMode::Off);
        assert!(!settings.performance_mode.reduces_motion());
        assert!(!serde_json::to_string(&settings)?.contains("performance_mode"));
        let read: Settings = serde_json::from_str(r#"{"performance_mode":"on"}"#)?;
        assert!(read.performance_mode.reduces_motion());
        Ok(())
    }

    #[test]
    fn new_settings_start_on_glass_and_a_saved_material_is_kept() -> serde_json::Result<()> {
        assert_eq!(Settings::default().window_material, WindowMaterial::Glass);
        let read: Settings = serde_json::from_str("{}")?;
        assert_eq!(read.window_material, WindowMaterial::Glass);
        let saved: Settings = serde_json::from_str(r#"{"window_material":"Opaque"}"#)?;
        assert_eq!(saved.window_material, WindowMaterial::Opaque);
        Ok(())
    }

    #[test]
    fn the_api_discovery_file_is_on_until_turned_off() -> serde_json::Result<()> {
        assert_eq!(Settings::default().api_discovery_file, ApiDiscoveryFile::On);
        assert!(!serde_json::to_string(&Settings::default())?.contains("api_discovery_file"));
        let read: Settings = serde_json::from_str(r#"{"api_discovery_file":"off"}"#)?;
        assert_eq!(read.api_discovery_file, ApiDiscoveryFile::Off);
        assert!(serde_json::to_string(&read)?.contains(r#""api_discovery_file":"off""#));
        Ok(())
    }

    #[test]
    fn the_composer_height_is_normal_until_picked_and_stays_out_of_the_file(
    ) -> serde_json::Result<()> {
        assert!(!serde_json::to_string(&Settings::default())?.contains("composer_height"));
        let read: Settings = serde_json::from_str(r#"{"composer_height":"compact"}"#)?;
        assert_eq!(read.composer_height, ComposerHeight::Compact);
        let written = serde_json::to_string(&read)?;
        assert!(written.contains(r#""composer_height":"compact""#));
        Ok(())
    }

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
    fn an_old_terminal_theme_becomes_the_theme_of_its_appearance() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"terminal_theme":"Dracula"}"#).unwrap_or_default();
        let selection = settings.migrate_themes().clone();
        assert_eq!(selection.dark, ThemeId("dracula".to_owned()));
        assert_eq!(selection.light, ThemeId(KRAD_LIGHT_THEME.to_owned()));
        let written = serde_json::to_string(&settings).unwrap_or_default();
        assert!(!written.contains("terminal_theme"));
        assert!(written.contains("dracula"));
    }

    #[test]
    fn settings_roundtrip_and_old_files_get_defaults() -> serde_json::Result<()> {
        let mut settings = Settings {
            themes: Some(TerminalTheme::Dracula.as_selection()),
            default_harness: AgentPreset::Codex,
            copy_mode: CopyMode::Both,
            composer_height: ComposerHeight::Roomy,
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
            partial.terminal_font.ligatures,
            Ligatures::On,
            "fonts join ligatures unless turned off"
        );
        assert_eq!(
            partial.app_updates,
            AppUpdateCheck::Automatic,
            "older settings files check for app updates"
        );
        assert_eq!(
            partial.composer_height,
            ComposerHeight::Normal,
            "older settings files get the normal composer"
        );
        Ok(())
    }

    #[test]
    fn the_last_model_and_effort_of_each_harness_are_remembered() -> serde_json::Result<()> {
        let mut settings = Settings::default();
        settings.chat_defaults.insert(
            AgentPreset::Codex,
            ChatChoice {
                model: ModelId::parse("gpt-6.1-sol"),
                effort: Some(crate::model::Effort::Ultra),
            },
        );
        let back: Settings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back.chat_defaults, settings.chat_defaults);
        let old: Settings = serde_json::from_str(r#"{"default_harness":"ClaudeCode"}"#)?;
        assert!(old.chat_defaults.is_empty());
        assert!(!serde_json::to_string(&old)?.contains("chat_defaults"));
        Ok(())
    }

    #[test]
    fn favorite_and_default_models_survive_a_restart() -> serde_json::Result<()> {
        let Some(fable) = ModelId::parse("fable") else {
            unreachable!("fable is a model id");
        };
        let mut settings = Settings::default();
        settings
            .model_preferences
            .toggle_favorite(AgentPreset::ClaudeCode, &fable);
        settings
            .model_preferences
            .toggle_default(AgentPreset::ClaudeCode, &fable);
        let back: Settings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back.model_preferences, settings.model_preferences);
        let old: Settings = serde_json::from_str(r#"{"default_harness":"ClaudeCode"}"#)?;
        assert!(old.model_preferences.is_empty());
        assert!(!serde_json::to_string(&old)?.contains("model_preferences"));
        Ok(())
    }

    #[test]
    fn acp_agents_open_their_terminal_command_and_old_files_have_none() -> serde_json::Result<()> {
        let old: Settings = serde_json::from_str(
            r#"{"default_harness":"ClaudeCode","harness_commands":{"Opencode":"opencode --port 4096"},"harness_models":{"ClaudeCode":["opus"]},"mcp":{}}"#,
        )?;
        assert_eq!(old.default_harness, AgentPreset::ClaudeCode);
        assert!(old.acp.is_empty());
        assert!(!serde_json::to_string(&old)?.contains("acp"));
        let codex_acp = AgentPreset::Acp(
            "codex-acp"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid id")),
        );
        assert_eq!(old.launch_line(codex_acp).as_deref(), Some("codex"));
        let mut settings = Settings {
            default_harness: codex_acp,
            ..old
        };
        let added = serde_json::from_str(
            r#"{"id":"gemini","name":"Gemini","program":"gemini","args":["--experimental-acp"]}"#,
        )?;
        assert_eq!(settings.acp.add_agent(added), Ok(()));
        let gemini = AgentPreset::Acp(
            "gemini"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid id")),
        );
        assert_eq!(settings.launch_line(gemini), None, "no terminal command");
        let back: Settings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back, settings);
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
    fn harness_onboarding_stays_out_of_the_file_until_it_is_done() -> serde_json::Result<()> {
        let fresh = serde_json::to_string(&Settings::default())?;
        assert!(!fresh.contains("harness_onboarding"));
        let old: Settings = serde_json::from_str(r#"{"appearance":"Dark"}"#)?;
        assert_eq!(old.harness_onboarding, HarnessOnboarding::Pending);
        let done = Settings {
            harness_onboarding: HarnessOnboarding::Done,
            ..Settings::default()
        };
        let read: Settings = serde_json::from_str(&serde_json::to_string(&done)?)?;
        assert_eq!(read.harness_onboarding, HarnessOnboarding::Done);
        Ok(())
    }

    #[test]
    fn theme_tuning_and_environment_themes_survive_a_restart() -> serde_json::Result<()> {
        let plain = serde_json::to_string(&Settings::default())?;
        assert!(!plain.contains("theme_tuning"));
        assert!(!plain.contains("environment_themes"));
        let work = EnvironmentName::new("work".to_owned());
        let mut settings = Settings::default();
        settings.theme_tuning.interface_scale = crate::InterfaceScale::new(120);
        settings.theme_tuning.main_pane = crate::MainPaneMaterial::Opaque;
        settings.environment_themes.set(
            &work,
            ThemeAppearance::Dark,
            Some(ThemeId("nord".to_owned())),
        );
        let back: Settings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back, settings);
        assert_eq!(
            back.theme_selection(Some(&work)).dark,
            ThemeId("nord".to_owned())
        );
        assert_eq!(back.theme_selection(None), ThemeSelection::default());
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
