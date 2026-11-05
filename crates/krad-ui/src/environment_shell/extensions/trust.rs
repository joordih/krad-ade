use gpui_kit::{div, px, rgb, AnyElement, Context, ParentElement, Styled, Window};
use krad_domain::ExtensionFingerprint;
use krad_extensions::api::manifest::{ExtensionManifest, Launch, Permission, Placement};
use krad_extensions::api::ExtensionId;
use krad_extensions::{ManifestSummary, Trust};

use super::super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title};
use super::super::EnvironmentShell;
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_1, SPACE_5, SPACE_6, TEXT_SM};

const SHEET_W: f32 = 560.0;
const TRUST_TITLE: &str = "Trust this extension?";
const TRUST_SUBTITLE: &str =
    "What it brings runs with your permissions and can do anything you can.";
const TRUST_LABEL: &str = "Trust";
const DECLINE_LABEL: &str = "Don't trust";
const BTN_TRUST: &str = "extension-trust";
const BTN_DECLINE: &str = "extension-decline";
const BTN_CLOSE: &str = "extension-trust-close";
const RUNS_APP: &str = "Runs a program once on this computer";
const RUNS_ENVIRONMENT: &str = "Runs a program once in each open environment, on its host";
const LAUNCH_RUNTIME: &str = "Starts with";
const LAUNCH_BINARY: &str = "Starts its own program";
const TERMINALS_WARNING: &str =
    "May open terminals running any command in your environments, including SSH hosts";
const RUN_COMMANDS_WARNING: &str = "May run the commands of your other extensions";
const HOST_KINDS_WARNING: &str =
    "Runs the command it chooses on this computer to reach the machines it offers";
const COMMANDS: &str = "Commands";
const VIEWS: &str = "Views";
const MENUS: &str = "Menu entries";
const KEYBINDINGS: &str = "Keyboard shortcuts";
const TOOLS: &str = "Tools for agents";
const HOST_KINDS: &str = "Host kinds";
const SECRETS: &str = "Secrets it keeps";
const LANGUAGES: &str = "Languages it adds or changes";
const SERVERS: &str = "Language servers it adds or changes";
const SERVER_RUNTIMES: &str = "Server runtimes it adds or changes, which the app runs";
const TOOLCHAINS: &str = "Toolchains it adds or changes";
const LOCATORS: &str = "Toolchain locators it adds or changes";
const LIST_SEPARATOR: &str = ", ";
const MAX_SHOWN_CHARS: usize = 80;
const ELLIPSIS: char = '…';

pub(super) fn plain(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = flat.trim();
    if trimmed.chars().count() <= MAX_SHOWN_CHARS {
        return trimmed.to_owned();
    }
    let mut cut: String = trimmed.chars().take(MAX_SHOWN_CHARS).collect();
    cut.push(ELLIPSIS);
    cut
}

fn program_lines(manifest: &ExtensionManifest, lines: &mut Vec<String>) {
    let Some(program) = &manifest.program else {
        return;
    };
    lines.push(
        match program.placement {
            Placement::App => RUNS_APP,
            Placement::Environment => RUNS_ENVIRONMENT,
        }
        .to_owned(),
    );
    lines.push(match &program.launch {
        Launch::Runtime { runtime, args } => {
            plain(&format!("{LAUNCH_RUNTIME} {runtime} {}", args.join(" ")))
        }
        Launch::Binary { .. } => LAUNCH_BINARY.to_owned(),
    });
    for permission in &program.permissions {
        lines.push(
            match permission {
                Permission::Terminals => TERMINALS_WARNING,
                Permission::RunCommands => RUN_COMMANDS_WARNING,
            }
            .to_owned(),
        );
    }
    if !manifest.contributes.host_kinds.is_empty() {
        lines.push(HOST_KINDS_WARNING.to_owned());
    }
}

fn declared_lines(manifest: Option<&ExtensionManifest>, summary: &ManifestSummary) -> Vec<String> {
    let mut lines = Vec::new();
    let mut list = |label: &str, names: Vec<String>| {
        if !names.is_empty() {
            let names: Vec<String> = names.iter().map(|name| plain(name)).collect();
            lines.push(format!("{label}: {}", names.join(LIST_SEPARATOR)));
        }
    };
    if let Some(manifest) = manifest {
        let contributes = &manifest.contributes;
        list(
            COMMANDS,
            contributes
                .commands
                .iter()
                .map(|c| c.title.clone())
                .collect(),
        );
        list(
            VIEWS,
            contributes.views.iter().map(|v| v.title.clone()).collect(),
        );
        list(
            MENUS,
            contributes
                .menus
                .iter()
                .map(|m| m.command.to_string())
                .collect(),
        );
        list(
            KEYBINDINGS,
            contributes
                .keybindings
                .iter()
                .map(|b| b.command.to_string())
                .collect(),
        );
        list(
            TOOLS,
            contributes
                .tools
                .iter()
                .map(|t| t.name.to_string())
                .collect(),
        );
        list(
            HOST_KINDS,
            contributes
                .host_kinds
                .iter()
                .map(|k| k.title.clone())
                .collect(),
        );
        list(
            SECRETS,
            contributes
                .secrets
                .iter()
                .map(|s| s.title.clone())
                .collect(),
        );
    }
    list(LANGUAGES, summary.languages.clone());
    list(SERVERS, summary.servers.clone());
    list(SERVER_RUNTIMES, summary.server_runtimes.clone());
    list(TOOLCHAINS, summary.toolchains.clone());
    list(LOCATORS, summary.locators.clone());
    let mut all = Vec::new();
    if let Some(manifest) = manifest {
        program_lines(manifest, &mut all);
    }
    all.extend(lines);
    all
}

impl EnvironmentShell {
    pub(super) fn queue_trust_prompts(&mut self, cx: &mut Context<Self>) {
        let asked: Vec<ExtensionId> = self
            .extensions
            .records
            .iter()
            .filter(|record| record.entry.enabled && record.trust() == Trust::Asked)
            .filter(|record| record.inspected().is_some())
            .map(|record| record.entry.id.clone())
            .collect();
        for id in asked {
            self.ask_trust(&id, cx);
        }
    }

    pub(in crate::environment_shell) fn ask_trust(
        &mut self,
        entry: &ExtensionId,
        cx: &mut Context<Self>,
    ) {
        let Some(fingerprint) = self
            .extensions
            .record(entry)
            .and_then(super::ExtensionRecord::fingerprint)
            .cloned()
        else {
            return;
        };
        let declined = self.extensions.declined.get(entry) == Some(&fingerprint);
        let queued = self
            .extensions
            .trust_queue
            .iter()
            .any(|(queued, _)| queued == entry);
        if declined || queued {
            return;
        }
        self.extensions
            .trust_queue
            .push_back((entry.clone(), fingerprint));
        cx.notify();
    }

    pub(in crate::environment_shell) fn review_trust(
        &mut self,
        entry: &ExtensionId,
        cx: &mut Context<Self>,
    ) {
        self.extensions.declined.remove(entry);
        self.ask_trust(entry, cx);
    }

    fn answer_trust(&mut self, trusted: bool, cx: &mut Context<Self>) {
        let Some((entry, shown)) = self.extensions.trust_queue.pop_front() else {
            return;
        };
        if !trusted {
            self.extensions.declined.insert(entry, shown);
            cx.notify();
            return;
        }
        let current = self
            .extensions
            .record(&entry)
            .and_then(super::ExtensionRecord::fingerprint);
        if current != Some(&shown) {
            self.ask_trust(&entry, cx);
            return;
        }
        if !store_trust(&mut self.settings.extensions, &entry, &shown) {
            return;
        }
        self.save_settings(cx);
        self.load_language_tools(cx);
        self.load_extensions(cx);
        cx.notify();
    }

    fn dismiss_trust(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.answer_trust(false, cx);
    }

    pub(in crate::environment_shell) fn trust_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (entry, shown) = self.extensions.trust_queue.front()?;
        let record = self.extensions.record(entry)?;
        let inspected = record.inspected()?;
        if inspected.fingerprint != *shown {
            return None;
        }
        let pal = palette(cx);
        let lines = declared_lines(inspected.manifest(), &inspected.summary)
            .into_iter()
            .map(|line| {
                div()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .child(line)
            });
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(
                format!("{TRUST_TITLE} {}", plain(&record.title())),
                Some(TRUST_SUBTITLE.to_owned()),
                BTN_CLOSE,
                Self::dismiss_trust,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1))
                    .px(px(SPACE_6))
                    .pb(px(SPACE_5))
                    .children(lines),
            )
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_DECLINE)
                            .label(DECLINE_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.answer_trust(false, cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_TRUST)
                            .label(TRUST_LABEL)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.answer_trust(true, cx);
                            })),
                    ),
            );
        Some(sheet_overlay(sheet, Self::dismiss_trust, cx))
    }
}

fn store_trust(
    entries: &mut [krad_domain::ExtensionEntry],
    entry: &ExtensionId,
    shown: &ExtensionFingerprint,
) -> bool {
    let Some(known) = entries.iter_mut().find(|known| known.id == *entry) else {
        return false;
    };
    known.trusted = Some(shown.clone());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extensions::api::manifest::{
        CommandContribution, Contributions, HostKindContribution, Program, ViewContribution,
        ViewZone,
    };
    use krad_extensions::api::{ApiVersion, IdError};

    fn docker() -> Result<ExtensionManifest, IdError> {
        Ok(ExtensionManifest {
            id: "docker".parse()?,
            name: "Docker".to_owned(),
            version: "0.1.0".to_owned(),
            description: None,
            api: ApiVersion::new(1, 0),
            program: Some(Program {
                placement: Placement::App,
                permissions: vec![Permission::Terminals],
                launch: Launch::Runtime {
                    runtime: "node".parse()?,
                    args: vec!["main.js".to_owned()],
                },
            }),
            activation: Vec::new(),
            contributes: Contributions {
                commands: vec![CommandContribution {
                    id: "refresh".parse()?,
                    title: "Docker: Refresh".to_owned(),
                    icon: None,
                }],
                views: vec![ViewContribution {
                    id: "containers".parse()?,
                    title: "Containers".to_owned(),
                    zone: ViewZone::Sidebar,
                    icon: None,
                }],
                host_kinds: vec![HostKindContribution {
                    id: "container".parse()?,
                    title: "Container".to_owned(),
                    icon: None,
                }],
                ..Contributions::default()
            },
        })
    }

    #[test]
    fn the_trust_sheet_lists_where_it_runs_what_it_may_do_and_what_it_adds() -> Result<(), IdError>
    {
        let lines = declared_lines(Some(&docker()?), &ManifestSummary::default());
        assert_eq!(lines.first().map(String::as_str), Some(RUNS_APP));
        assert!(lines.iter().any(|line| line == "Starts with node main.js"));
        assert!(lines.iter().any(|line| line == TERMINALS_WARNING));
        assert!(lines.iter().any(|line| line == HOST_KINDS_WARNING));
        assert!(lines.iter().any(|line| line == "Commands: Docker: Refresh"));
        assert!(lines.iter().any(|line| line == "Views: Containers"));
        Ok(())
    }

    #[test]
    fn a_manifest_only_extension_lists_the_runtimes_it_would_change() {
        let summary = ManifestSummary {
            server_runtimes: vec!["node".to_owned()],
            languages: vec!["zig".to_owned()],
            ..ManifestSummary::default()
        };
        let lines = declared_lines(None, &summary);
        assert!(lines
            .iter()
            .any(|line| line.starts_with(SERVER_RUNTIMES) && line.ends_with("node")));
        assert!(lines.iter().any(|line| line.ends_with("zig")));
    }

    #[test]
    fn names_cannot_hide_lines_or_run_on_forever() {
        assert_eq!(plain("Docker\nTrusted by Krad"), "Docker Trusted by Krad");
        let long = "x".repeat(MAX_SHOWN_CHARS * 2);
        assert_eq!(plain(&long).chars().count(), MAX_SHOWN_CHARS + 1);
    }

    #[test]
    fn trust_is_stored_only_for_the_extension_that_was_shown(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let entry = |id: &str| -> Result<krad_domain::ExtensionEntry, IdError> {
            Ok(krad_domain::ExtensionEntry {
                id: id.parse()?,
                source: krad_domain::ExtensionSource::Folder("/x".into()),
                enabled: true,
                trusted: None,
            })
        };
        let mut entries = vec![entry("docker")?, entry("other")?];
        let shown = ExtensionFingerprint("seen".to_owned());
        assert!(store_trust(&mut entries, &"docker".parse()?, &shown));
        assert_eq!(entries[0].trusted.as_ref(), Some(&shown));
        assert_eq!(entries[1].trusted, None);
        assert!(!store_trust(&mut entries, &"missing".parse()?, &shown));
        Ok(())
    }
}
