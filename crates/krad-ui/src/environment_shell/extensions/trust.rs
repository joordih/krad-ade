use gpui_kit::{div, px, rgb, AnyElement, Context, ParentElement, Styled, Window};
use krad_extensions::api::manifest::{ExtensionManifest, Launch, Placement};
use krad_extensions::program::Trust;

use super::super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title};
use super::super::EnvironmentShell;
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_1, SPACE_5, SPACE_6, TEXT_SM};

const SHEET_W: f32 = 560.0;
const TRUST_TITLE: &str = "Trust this extension?";
const TRUST_SUBTITLE: &str = "Its program runs with your permissions and can do anything you can.";
const TRUST_LABEL: &str = "Trust";
const DECLINE_LABEL: &str = "Don't trust";
const BTN_TRUST: &str = "extension-trust";
const BTN_DECLINE: &str = "extension-decline";
const BTN_CLOSE: &str = "extension-trust-close";
const RUNS_APP: &str = "Runs once on this computer";
const RUNS_ENVIRONMENT: &str = "Runs once in each open environment, on its host";
const LAUNCH_RUNTIME: &str = "Starts with";
const LAUNCH_BINARY: &str = "Starts its own program";
const COMMANDS: &str = "Commands";
const VIEWS: &str = "Views";
const MENUS: &str = "Menu entries";
const KEYBINDINGS: &str = "Keyboard shortcuts";
const TOOLS: &str = "Tools for agents";
const HOST_KINDS: &str = "Host kinds";
const SECRETS: &str = "Secrets it keeps";
const LIST_SEPARATOR: &str = ", ";

fn declared_lines(manifest: &ExtensionManifest) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(program) = &manifest.program {
        lines.push(
            match program.placement {
                Placement::App => RUNS_APP,
                Placement::Environment => RUNS_ENVIRONMENT,
            }
            .to_owned(),
        );
        lines.push(match &program.launch {
            Launch::Runtime { runtime, args } => {
                format!("{LAUNCH_RUNTIME} {runtime} {}", args.join(" "))
            }
            Launch::Binary { .. } => LAUNCH_BINARY.to_owned(),
        });
    }
    let contributes = &manifest.contributes;
    let mut list = |label: &str, names: Vec<String>| {
        if !names.is_empty() {
            lines.push(format!("{label}: {}", names.join(LIST_SEPARATOR)));
        }
    };
    list(
        COMMANDS,
        contributes
            .commands
            .iter()
            .map(|command| command.title.clone())
            .collect(),
    );
    list(
        VIEWS,
        contributes
            .views
            .iter()
            .map(|view| view.title.clone())
            .collect(),
    );
    list(
        MENUS,
        contributes
            .menus
            .iter()
            .map(|menu| menu.command.to_string())
            .collect(),
    );
    list(
        KEYBINDINGS,
        contributes
            .keybindings
            .iter()
            .map(|binding| binding.command.to_string())
            .collect(),
    );
    list(
        TOOLS,
        contributes
            .tools
            .iter()
            .map(|tool| tool.name.to_string())
            .collect(),
    );
    list(
        HOST_KINDS,
        contributes
            .host_kinds
            .iter()
            .map(|kind| kind.title.clone())
            .collect(),
    );
    list(
        SECRETS,
        contributes
            .secrets
            .iter()
            .map(|secret| secret.title.clone())
            .collect(),
    );
    lines
}

impl EnvironmentShell {
    pub(super) fn queue_trust_prompts(&mut self, cx: &mut Context<Self>) {
        let asked: Vec<String> = self
            .extensions
            .records
            .iter()
            .filter(|record| record.entry.enabled && record.trust() == Trust::Asked)
            .map(|record| record.entry.id.clone())
            .collect();
        for id in asked {
            self.ask_trust(&id, cx);
        }
    }

    pub(in crate::environment_shell) fn ask_trust(&mut self, entry: &str, cx: &mut Context<Self>) {
        if self.extensions.declined.contains(entry)
            || self
                .extensions
                .trust_queue
                .iter()
                .any(|queued| queued == entry)
        {
            return;
        }
        self.extensions.trust_queue.push_back(entry.to_owned());
        cx.notify();
    }

    pub(in crate::environment_shell) fn review_trust(
        &mut self,
        entry: &str,
        cx: &mut Context<Self>,
    ) {
        self.extensions.declined.remove(entry);
        self.ask_trust(entry, cx);
    }

    fn answer_trust(&mut self, trusted: bool, cx: &mut Context<Self>) {
        let Some(entry) = self.extensions.trust_queue.pop_front() else {
            return;
        };
        if !trusted {
            self.extensions.declined.insert(entry);
            cx.notify();
            return;
        }
        let fingerprint = self
            .extensions
            .records
            .iter()
            .find(|record| record.entry.id == entry)
            .and_then(super::ExtensionRecord::installed)
            .map(krad_extensions::program::InstalledExtension::fingerprint);
        let Some(fingerprint) = fingerprint else {
            return;
        };
        for known in &mut self.settings.extensions {
            if known.id == entry {
                known.trusted = Some(fingerprint.clone());
            }
        }
        for record in &mut self.extensions.records {
            if record.entry.id == entry {
                record.entry.trusted = Some(fingerprint.clone());
            }
        }
        self.save_settings(cx);
        self.bind_extension_keys(cx);
        self.activate_on_startup(cx);
        cx.notify();
    }

    fn dismiss_trust(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.answer_trust(false, cx);
    }

    pub(in crate::environment_shell) fn trust_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let entry = self.extensions.trust_queue.front()?;
        let record = self
            .extensions
            .records
            .iter()
            .find(|record| record.entry.id == *entry)?;
        let manifest = record.manifest()?;
        let pal = palette(cx);
        let lines = declared_lines(manifest).into_iter().map(|line| {
            div()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_default))
                .child(line)
        });
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(
                format!("{TRUST_TITLE} {}", manifest.name),
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

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extensions::api::manifest::{
        CommandContribution, Contributions, Program, ViewContribution, ViewZone,
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
                ..Contributions::default()
            },
        })
    }

    #[test]
    fn the_trust_sheet_lists_where_it_runs_and_what_it_adds() -> Result<(), IdError> {
        let lines = declared_lines(&docker()?);
        assert_eq!(lines.first().map(String::as_str), Some(RUNS_APP));
        assert!(lines.iter().any(|line| line == "Starts with node main.js"));
        assert!(lines.iter().any(|line| line == "Commands: Docker: Refresh"));
        assert!(lines.iter().any(|line| line == "Views: Containers"));
        Ok(())
    }
}
