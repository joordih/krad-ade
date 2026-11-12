use crate::components::IconName;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::environment_shell) enum SettingsPane {
    General,
    Notifications,
    Appearance,
    Terminal,
    Editor,
    Chat,
    Harnesses,
    Gateway,
    Acp,
    Mcp,
    Skills,
    CloudRunners,
    Worktrees,
    Integrations,
    Webhooks,
    Languages,
    Extensions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsSection {
    App,
    Agents,
    Workspace,
}

impl SettingsSection {
    pub(super) const ALL: [Self; 3] = [Self::App, Self::Agents, Self::Workspace];

    pub(super) const fn title(self) -> &'static str {
        match self {
            Self::App => "App",
            Self::Agents => "Agents",
            Self::Workspace => "Workspace",
        }
    }

    pub(super) const fn panes(self) -> &'static [SettingsPane] {
        match self {
            Self::App => &[
                SettingsPane::General,
                SettingsPane::Notifications,
                SettingsPane::Appearance,
                SettingsPane::Terminal,
                SettingsPane::Editor,
            ],
            Self::Agents => &[
                SettingsPane::Chat,
                SettingsPane::Harnesses,
                SettingsPane::Gateway,
                SettingsPane::Acp,
                SettingsPane::Mcp,
                SettingsPane::Skills,
                SettingsPane::CloudRunners,
            ],
            Self::Workspace => &[
                SettingsPane::Worktrees,
                SettingsPane::Integrations,
                SettingsPane::Webhooks,
                SettingsPane::Languages,
                SettingsPane::Extensions,
            ],
        }
    }
}

impl SettingsPane {
    pub(super) const fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Notifications => "Notifications",
            Self::Appearance => "Appearance",
            Self::Terminal => "Terminal",
            Self::Editor => "Editor",
            Self::Chat => "Chat",
            Self::Harnesses => "Harnesses",
            Self::Gateway => "Gateway",
            Self::Acp => "ACP agents",
            Self::Mcp => "MCP",
            Self::Skills => "Skills",
            Self::CloudRunners => "Cloud runners",
            Self::Worktrees => "Worktrees",
            Self::Integrations => "Integrations",
            Self::Webhooks => "Webhooks",
            Self::Languages => "Languages",
            Self::Extensions => "Extensions",
        }
    }

    pub(super) const fn description(self) -> &'static str {
        match self {
            Self::General => "App updates and how much the app animates.",
            Self::Notifications => {
                "When agents finish or need you: notifications, sounds, quiet hours and the app icon."
            }
            Self::Appearance => "Light and dark themes, the window and the interface font.",
            Self::Terminal => "The font, copy and paste, and the colours of terminals.",
            Self::Editor => "How files look, show their problems and the minimap.",
            Self::Chat => {
                "The tools agents get, the chat composer, what agents may spend and plan usage."
            }
            Self::Harnesses => {
                "Whether Claude Code, Codex and opencode are installed, current and signed in."
            }
            Self::Gateway => {
                "Chat with the models of a gateway you run, through the app's own agent."
            }
            Self::Acp => "Agents the app talks to over the Agent Client Protocol.",
            Self::Mcp => "The MCP servers whose tools Gateway chats can use.",
            Self::Skills => "The skills agents can call, and which of them are on.",
            Self::CloudRunners => {
                "Linux machines over SSH where sessions keep running while this computer sleeps."
            }
            Self::Worktrees => {
                "Where new worktrees go, what they start from, and when finished ones are removed."
            }
            Self::Integrations => "The code hosts reviews and merge requests come from.",
            Self::Webhooks => "Send the app's events to your own services over HTTPS.",
            Self::Languages => "Language servers and the SDKs they need.",
            Self::Extensions => {
                "Extensions that add languages, commands, views and tools, and their settings."
            }
        }
    }

    pub(super) const fn icon(self) -> IconName {
        match self {
            Self::General => IconName::Settings,
            Self::Notifications => IconName::Bell,
            Self::Appearance => IconName::Sun,
            Self::Terminal => IconName::Terminal,
            Self::Editor => IconName::File,
            Self::Chat => IconName::Chat,
            Self::Harnesses => IconName::CheckCircle,
            Self::Gateway => IconName::Flow,
            Self::Acp => IconName::UserGroup,
            Self::Mcp => IconName::Plug,
            Self::Skills => IconName::Magic,
            Self::CloudRunners => IconName::Cloud,
            Self::Worktrees => IconName::GitBranch,
            Self::Integrations => IconName::Merge,
            Self::Webhooks => IconName::Send,
            Self::Languages => IconName::Workflow,
            Self::Extensions => IconName::Layers,
        }
    }

    pub(super) const fn settings(self) -> &'static [&'static str] {
        match self {
            Self::General => &["Check for updates", "Performance mode", "Animations"],
            Self::Notifications => &[
                "System notifications",
                "Answer from notifications",
                "Events",
                "Sounds",
                "Quiet hours",
                "App icon",
                "Shortcuts",
            ],
            Self::Appearance => &[
                "Light theme",
                "Dark theme",
                "Create theme",
                "Window material",
                "Tint color",
                "Tint opacity",
                "Environment theme",
                "Accent color",
                "Hue",
                "Saturation",
                "Dark mode lightness",
                "Sidebar opacity",
                "Main pane glass",
                "Diff colors",
                "Interface scale",
                "Interface font",
            ],
            Self::Terminal => &["Copy", "Paste", "Terminal font", "Ligatures"],
            Self::Editor => &[
                "Editor font",
                "Problem lens",
                "Minimap",
                "Project check limit",
                "File icon colors",
            ],
            Self::Chat => &[
                "Code tools for agents",
                "Extension tools for agents",
                "Let other programs find the agents API",
                "Chat composer height",
                "Budget per group",
                "Spending",
                "Plan usage",
                "Usage bar",
            ],
            Self::Harnesses => &["Agent CLIs", "Claude Code", "Codex", "opencode", "Sign in"],
            Self::Gateway => &["Connection", "Address", "Key", "Models"],
            Self::Acp => &["Known agents", "Your agents", "Add agent", "Packages"],
            Self::Mcp => &["Servers", "Add server", "Project servers", "Tools"],
            Self::Skills => &["Skill library", "Enabled skills"],
            Self::CloudRunners => &["Add cloud runner", "Reconnect", "Lingering", "SSH"],
            Self::Worktrees => &[
                "Remove merged worktrees",
                "Worktree folder",
                "Branch prefix",
                "Base branch",
                "Fetch first",
                "Copy env files",
                "Confirm removal",
                "Existing worktrees",
            ],
            Self::Integrations => &["Accounts", "GitLab", "GitHub", "Your groups"],
            Self::Webhooks => &["Add webhook", "Send test", "Events"],
            Self::Languages => &[
                "Language servers",
                "Server runtimes",
                "Toolchains",
                "Activate in new terminals",
                "Java engine",
            ],
            Self::Extensions => &["Installed", "Browse", "Skipped shortcuts", "Program log"],
        }
    }

    pub(super) fn section(self) -> SettingsSection {
        SettingsSection::ALL
            .into_iter()
            .find(|section| section.panes().contains(&self))
            .unwrap_or(SettingsSection::App)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PaneMatch {
    pub(super) pane: SettingsPane,
    pub(super) settings: Vec<&'static str>,
}

fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

fn holds_every(text: &str, words: &[String]) -> bool {
    let text = text.to_lowercase();
    words.iter().all(|word| text.contains(word.as_str()))
}

#[must_use]
pub(super) fn search(query: &str) -> Vec<PaneMatch> {
    let words = words(query);
    SettingsSection::ALL
        .into_iter()
        .flat_map(SettingsSection::panes)
        .filter_map(|&pane| {
            let settings: Vec<&'static str> = pane
                .settings()
                .iter()
                .copied()
                .filter(|setting| holds_every(setting, &words))
                .collect();
            let whole = std::iter::once(pane.title())
                .chain(std::iter::once(pane.description()))
                .chain(pane.settings().iter().copied())
                .collect::<Vec<_>>()
                .join(" ");
            (!settings.is_empty() || holds_every(&whole, &words))
                .then_some(PaneMatch { pane, settings })
        })
        .collect()
}

#[must_use]
pub(super) fn next_pane(shown: SettingsPane, panes: &[SettingsPane], step: isize) -> SettingsPane {
    let Some(at) = panes.iter().position(|pane| *pane == shown) else {
        return panes.first().copied().unwrap_or(shown);
    };
    let last = panes.len().saturating_sub(1);
    let next = at.saturating_add_signed(step).min(last);
    panes.get(next).copied().unwrap_or(shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_pane() -> Vec<SettingsPane> {
        SettingsSection::ALL
            .into_iter()
            .flat_map(SettingsSection::panes)
            .copied()
            .collect()
    }

    #[test]
    fn every_page_sits_in_one_section_with_its_own_title_and_icon() {
        let panes = every_pane();
        for (index, pane) in panes.iter().enumerate() {
            assert_eq!(
                pane.section().panes().iter().filter(|p| *p == pane).count(),
                1
            );
            for other in &panes[index + 1..] {
                assert!(pane != other, "{} sits in two sections", pane.title());
                assert_ne!(pane.title(), other.title());
                assert_ne!(pane.icon(), other.icon());
            }
        }
        for pane in [
            SettingsPane::Skills,
            SettingsPane::Worktrees,
            SettingsPane::Chat,
        ] {
            assert!(panes.contains(&pane), "{} has no place", pane.title());
        }
        assert_eq!(SettingsPane::Skills.section(), SettingsSection::Agents);
        assert_eq!(
            SettingsPane::Worktrees.section(),
            SettingsSection::Workspace
        );
    }

    #[test]
    fn an_empty_search_lists_every_page() {
        let all: Vec<SettingsPane> = search("  ").into_iter().map(|hit| hit.pane).collect();
        assert_eq!(all, every_pane());
    }

    #[test]
    fn a_search_finds_settings_on_the_page_they_moved_to() {
        let hits = search("composer");
        assert_eq!(
            hits,
            [PaneMatch {
                pane: SettingsPane::Chat,
                settings: vec!["Chat composer height"],
            }]
        );
        let hits = search("BRANCH prefix");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].pane, SettingsPane::Worktrees);
        assert_eq!(hits[0].settings, ["Branch prefix"]);
    }

    #[test]
    fn a_search_matching_a_page_keeps_it_even_without_a_matching_setting() {
        let hits = search("webhooks");
        assert!(hits.iter().any(|hit| hit.pane == SettingsPane::Webhooks));
        assert_eq!(search("nothing-matches-this"), Vec::<PaneMatch>::new());
    }

    #[test]
    fn arrow_keys_walk_the_listed_pages_and_stop_at_the_ends() {
        let panes = [
            SettingsPane::General,
            SettingsPane::Chat,
            SettingsPane::Worktrees,
        ];
        assert_eq!(
            next_pane(SettingsPane::General, &panes, 1),
            SettingsPane::Chat
        );
        assert_eq!(
            next_pane(SettingsPane::Worktrees, &panes, 1),
            SettingsPane::Worktrees
        );
        assert_eq!(
            next_pane(SettingsPane::General, &panes, -1),
            SettingsPane::General
        );
        assert_eq!(
            next_pane(SettingsPane::Editor, &panes, 1),
            SettingsPane::General
        );
    }
}
