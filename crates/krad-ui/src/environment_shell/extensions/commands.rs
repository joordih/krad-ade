use gpui_kit::{Context, Window};
use krad_extensions::api::manifest::{MenuTarget, ViewContribution};
use krad_extensions::api::protocol::CommandTarget;
use krad_extensions::api::{ExtensionId, Icon, QualifiedCommand, ViewId};
use std::path::Path;

use super::super::context_menu::{MenuAction, MenuEntry};
use super::super::EnvironmentShell;
use super::matching::menu_applies;
use super::prompts::PromptAnswer;
use super::Deferred;
use crate::actions::RunExtensionCommand;
use crate::components::IconName;

const OPEN_VIEW_PREFIX: &str = "Open";

#[derive(Clone, Debug, PartialEq)]
pub(in crate::environment_shell) enum ExtensionMenu {
    Run(QualifiedCommand, Option<CommandTarget>),
    Answer(PromptAnswer),
}

#[derive(Clone, Debug)]
pub(in crate::environment_shell) enum ExtensionPick {
    Run(QualifiedCommand),
    OpenView(ExtensionId, ViewId),
}

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct PaletteEntry {
    pub(in crate::environment_shell) label: String,
    pub(in crate::environment_shell) icon: IconName,
    pub(in crate::environment_shell) pick: ExtensionPick,
}

impl PaletteEntry {
    pub(in crate::environment_shell) fn shortcut(&self) -> Option<Box<dyn gpui_kit::Action>> {
        match &self.pick {
            ExtensionPick::Run(command) => Some(Box::new(RunExtensionCommand {
                extension: command.extension.clone(),
                command: command.command.clone(),
            })),
            ExtensionPick::OpenView(..) => None,
        }
    }
}

pub(in crate::environment_shell) const fn icon_of(icon: Option<Icon>) -> IconName {
    let Some(icon) = icon else {
        return IconName::Plug;
    };
    match icon {
        Icon::Add => IconName::Add,
        Icon::Attach => IconName::Attach,
        Icon::Bell => IconName::Bell,
        Icon::Book => IconName::Book,
        Icon::Cancel => IconName::CancelCircle,
        Icon::Check => IconName::CheckCircle,
        Icon::ChevronDown => IconName::ChevronDown,
        Icon::ChevronLeft => IconName::ChevronLeft,
        Icon::ChevronRight => IconName::ChevronRight,
        Icon::ChevronUp => IconName::ChevronUp,
        Icon::Close => IconName::Close,
        Icon::Copy => IconName::Copy,
        Icon::Exchange => IconName::Exchange,
        Icon::Expand => IconName::Expand,
        Icon::File => IconName::File,
        Icon::Flag => IconName::Flag,
        Icon::Flash => IconName::Flash,
        Icon::Flow => IconName::Flow,
        Icon::Folder => IconName::Folder,
        Icon::GitBranch => IconName::GitBranch,
        Icon::Key => IconName::Key,
        Icon::Layers => IconName::Layers,
        Icon::ListView => IconName::ListView,
        Icon::Loading => IconName::Loading,
        Icon::Lock => IconName::Lock,
        Icon::Magic => IconName::Magic,
        Icon::Minus => IconName::Minus,
        Icon::More => IconName::More,
        Icon::Note => IconName::Note,
        Icon::OpenExternal => IconName::OpenExternal,
        Icon::Pencil => IconName::Pencil,
        Icon::Play => IconName::Play,
        Icon::Plug => IconName::Plug,
        Icon::Refresh => IconName::Refresh,
        Icon::Repeat => IconName::Repeat,
        Icon::Search => IconName::Search,
        Icon::Settings => IconName::Settings,
        Icon::Stop => IconName::Stop,
        Icon::Target => IconName::Target,
        Icon::Tasks => IconName::Tasks,
        Icon::Terminal => IconName::Terminal,
        Icon::TreeView => IconName::TreeView,
        Icon::Undo => IconName::Undo,
        Icon::Workflow => IconName::Workflow,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn extension_palette_entries(&self) -> Vec<PaletteEntry> {
        let mut entries = Vec::new();
        for manifest in self.usable_manifests() {
            entries.extend(
                manifest
                    .contributes
                    .commands
                    .iter()
                    .map(|command| PaletteEntry {
                        label: command.title.clone(),
                        icon: icon_of(command.icon),
                        pick: ExtensionPick::Run(QualifiedCommand {
                            extension: manifest.id.clone(),
                            command: command.id.clone(),
                        }),
                    }),
            );
            entries.extend(manifest.contributes.views.iter().map(|view| PaletteEntry {
                label: format!("{OPEN_VIEW_PREFIX} {}", view.title),
                icon: icon_of(view.icon),
                pick: ExtensionPick::OpenView(manifest.id.clone(), view.id.clone()),
            }));
        }
        entries
    }

    pub(in crate::environment_shell) fn apply_extension_pick(
        &mut self,
        pick: ExtensionPick,
        cx: &mut Context<Self>,
    ) {
        match pick {
            ExtensionPick::Run(command) => self.run_extension_command(command, None, cx),
            ExtensionPick::OpenView(extension, view) => {
                self.open_extension_view(&extension, &view, None, None, cx);
            }
        }
    }

    pub(in crate::environment_shell) fn declares_command(
        &self,
        command: &QualifiedCommand,
    ) -> bool {
        self.usable_manifests().iter().any(|manifest| {
            manifest.id == command.extension
                && manifest
                    .contributes
                    .commands
                    .iter()
                    .any(|declared| declared.id == command.command)
        })
    }

    pub(in crate::environment_shell) fn run_extension_command(
        &mut self,
        command: QualifiedCommand,
        target: Option<CommandTarget>,
        cx: &mut Context<Self>,
    ) {
        if !self.declares_command(&command) {
            return;
        }
        let Some(key) = self
            .extensions
            .record(&command.extension)
            .and_then(super::ExtensionRecord::manifest)
            .and_then(|manifest| self.slot_key(manifest))
        else {
            return;
        };
        self.ensure_running(key, Deferred::Command(command.command, target), cx);
    }

    pub(in crate::environment_shell) fn extension_menu_entries(
        &self,
        target: MenuTarget,
        path: Option<&Path>,
        command_target: &CommandTarget,
    ) -> Vec<MenuEntry> {
        let mut entries = Vec::new();
        for manifest in self.usable_manifests() {
            for menu in &manifest.contributes.menus {
                if !menu_applies(menu, target, path) {
                    continue;
                }
                let Some(command) = manifest
                    .contributes
                    .commands
                    .iter()
                    .find(|command| command.id == menu.command)
                else {
                    continue;
                };
                let run = ExtensionMenu::Run(
                    QualifiedCommand {
                        extension: manifest.id.clone(),
                        command: command.id.clone(),
                    },
                    Some(command_target.clone()),
                );
                entries.push(MenuEntry::item(
                    command.title.clone(),
                    Some(icon_of(command.icon)),
                    MenuAction::Extension(Box::new(run)),
                ));
            }
        }
        if !entries.is_empty() {
            entries.insert(0, MenuEntry::Separator);
        }
        entries
    }

    pub(in crate::environment_shell) fn extension_entry_menu(
        &self,
        path: &Path,
        folder: bool,
    ) -> Vec<MenuEntry> {
        let (target, command_target) = if folder {
            (
                MenuTarget::Folder,
                CommandTarget::Folder {
                    path: path.to_path_buf(),
                },
            )
        } else {
            (
                MenuTarget::File,
                CommandTarget::File {
                    path: path.to_path_buf(),
                },
            )
        };
        self.extension_menu_entries(target, Some(path), &command_target)
    }

    pub(in crate::environment_shell) fn run_extension_menu(
        &mut self,
        menu: ExtensionMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match menu {
            ExtensionMenu::Run(command, target) => self.run_extension_command(command, target, cx),
            ExtensionMenu::Answer(answer) => self.answer_prompt(answer, window, cx),
        }
    }

    pub(in crate::environment_shell) fn on_run_extension_command(
        &mut self,
        action: &RunExtensionCommand,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = QualifiedCommand {
            extension: action.extension.clone(),
            command: action.command.clone(),
        };
        self.run_extension_command(command, None, cx);
    }

    pub(in crate::environment_shell) fn declared_views(
        &self,
    ) -> Vec<(ExtensionId, ViewContribution)> {
        self.usable_manifests()
            .into_iter()
            .flat_map(|manifest| {
                let id = manifest.id.clone();
                manifest
                    .contributes
                    .views
                    .into_iter()
                    .map(move |view| (id.clone(), view))
            })
            .collect()
    }
}
