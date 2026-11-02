use gpui_kit::Context;
use krad_domain::{EffectiveToolchain, Toolchain, ToolchainChoice, ToolchainScope};
use krad_extensions::ToolchainFamilyManifest;
use std::path::Path;

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::language_servers::Document;
use super::super::EnvironmentShell;
use super::label::{row_detail, toolchain_title};
use crate::components::IconName;

const TOOLCHAINS_SUFFIX: &str = "toolchains";
const SEARCH_PLACEHOLDER: &str = "Search toolchains…";
const AUTOMATIC_LABEL: &str = "Automatic";
const ENTER_PATH_LABEL: &str = "Enter a path…";
const RESCAN_LABEL: &str = "Rescan";
const NO_FILE_TEXT: &str = "Open a file in the editor to choose its toolchain";
const PALETTE_PICKER_TOP: f32 = 120.0;
const PALETTE_PICKER_HALF_WIDTH: f32 = 160.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PickerGroup {
    pub(super) label: String,
    pub(super) toolchains: Vec<Toolchain>,
}

pub(super) fn picker_groups(
    family: &ToolchainFamilyManifest,
    candidates: &[Toolchain],
) -> Vec<PickerGroup> {
    let mut groups: Vec<PickerGroup> = Vec::new();
    for toolchain in candidates {
        let label = family.label(&toolchain.kind);
        match groups.iter_mut().find(|group| group.label == label) {
            Some(group) => group.toolchains.push(toolchain.clone()),
            None => groups.push(PickerGroup {
                label,
                toolchains: vec![toolchain.clone()],
            }),
        }
    }
    groups
}

fn none_found(family: &ToolchainFamilyManifest) -> String {
    format!(
        "No {} toolchain was found. Enter a path to one, such as a virtual environment or an SDK folder.",
        family.name
    )
}

fn automatic_label(family: &ToolchainFamilyManifest, best: Option<&Toolchain>) -> String {
    best.map_or_else(
        || AUTOMATIC_LABEL.to_owned(),
        |best| format!("{AUTOMATIC_LABEL} ({})", toolchain_title(family, best)),
    )
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_toolchain_picker(
        &mut self,
        scope: &ToolchainScope,
        origin: (f32, f32),
        placement: MenuPlacement,
        cx: &mut Context<Self>,
    ) {
        let Some(family) = self.family_manifest(&scope.family) else {
            return;
        };
        let entries = self.toolchain_picker_entries(scope, &family);
        self.open_menu(origin, placement, entries, cx);
        self.open_submenu(0, cx);
    }

    pub(in crate::environment_shell) fn select_focused_toolchain(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let scope = self.envs.get(&self.active).and_then(|live| {
            let editor = live.editors.get(&live.focused?)?;
            let document = Document::new(&live.data.host, &editor.path);
            self.language_families(&editor.path)
                .iter()
                .find_map(|family| self.toolchains.scope_of(&document, &family.id).cloned())
        });
        let Some(scope) = scope else {
            self.push_toast(NO_FILE_TEXT.to_owned());
            cx.notify();
            return;
        };
        let origin = (
            (self.canvas_px.0 / 2.0 - PALETTE_PICKER_HALF_WIDTH).max(0.0),
            PALETTE_PICKER_TOP,
        );
        self.open_toolchain_picker(&scope, origin, MenuPlacement::Below, cx);
    }

    fn scope_home(&self, scope: &ToolchainScope) -> Option<std::path::PathBuf> {
        let (name, _) = self
            .envs
            .iter()
            .find(|(_, live)| live.data.host == scope.host)?;
        self.env_home(name)
    }

    fn toolchain_picker_entries(
        &self,
        scope: &ToolchainScope,
        family: &ToolchainFamilyManifest,
    ) -> Vec<MenuEntry> {
        let effective = self.toolchains.effective(scope);
        let candidates = self.toolchains.candidates(scope);
        let home = self.scope_home(scope);
        let chosen = effective
            .as_ref()
            .filter(|effective| effective.is_chosen())
            .and_then(EffectiveToolchain::toolchain);
        let automatic = effective
            .as_ref()
            .is_some_and(|effective| matches!(effective, EffectiveToolchain::Automatic(_)));
        let mut list = vec![MenuEntry::checked(
            automatic_label(family, candidates.first()),
            automatic,
            MenuAction::AutomaticToolchain(Box::new(scope.clone())),
        )];
        if candidates.is_empty() {
            list.push(MenuEntry::Note(none_found(family)));
        }
        for group in picker_groups(family, candidates) {
            list.push(MenuEntry::Header(group.label));
            list.extend(group.toolchains.into_iter().map(|toolchain| {
                let detail = row_detail(&toolchain, home.as_deref().map(Path::new));
                MenuEntry::checked(
                    toolchain.name.clone(),
                    chosen.is_some_and(|chosen| chosen.is_same(&toolchain)),
                    MenuAction::ChooseToolchain(Box::new(ToolchainChoice {
                        scope: scope.clone(),
                        toolchain,
                    })),
                )
                .with_detail(detail)
            }));
        }
        vec![
            MenuEntry::submenu(
                format!("{} {TOOLCHAINS_SUFFIX}", family.name),
                IconName::Layers,
                list,
            )
            .searchable(SEARCH_PLACEHOLDER),
            MenuEntry::Separator,
            MenuEntry::item(
                ENTER_PATH_LABEL,
                Some(IconName::Folder),
                MenuAction::EnterToolchainPath(Box::new(scope.clone())),
            ),
            MenuEntry::item(
                RESCAN_LABEL,
                Some(IconName::Refresh),
                MenuAction::RescanToolchains(Box::new(scope.clone())),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{LocatorId, ToolchainFamilyId, ToolchainKind, ToolchainLocation};
    use krad_extensions::toolchain_manifest::KindLabel;
    use std::path::PathBuf;

    fn family(kinds: &[(&str, &str)]) -> ToolchainFamilyManifest {
        ToolchainFamilyManifest {
            id: ToolchainFamilyId::new("python"),
            name: "Python".to_owned(),
            languages: vec!["python".to_owned()],
            markers: Vec::new(),
            priority: Vec::new(),
            kinds: kinds
                .iter()
                .map(|(id, label)| KindLabel {
                    id: ToolchainKind::new(*id),
                    label: (*label).to_owned(),
                })
                .collect(),
            locators: Vec::new(),
            hints: Vec::new(),
            activation: Vec::new(),
            entered: Vec::new(),
        }
    }

    fn toolchain(kind: &str, path: &str) -> Toolchain {
        Toolchain {
            family: ToolchainFamilyId::new("python"),
            kind: ToolchainKind::new(kind),
            name: path.to_owned(),
            version: None,
            location: ToolchainLocation::Path {
                path: PathBuf::from(path),
            },
            prefix: None,
            project: None,
            locator: LocatorId::new("pet"),
        }
    }

    #[test]
    fn candidates_group_by_kind_label_in_ranked_order() {
        let family = family(&[
            ("venv", "Virtual environment"),
            ("conda", "Conda"),
            ("linux-global", "System"),
            ("system", "System"),
        ]);
        let candidates = [
            toolchain("venv", "/a/.venv/bin/python"),
            toolchain("linux-global", "/usr/bin/python3"),
            toolchain("conda", "/opt/conda/bin/python"),
            toolchain("system", "/usr/local/bin/python3"),
            toolchain("venv", "/b/.venv/bin/python"),
        ];
        let groups = picker_groups(&family, &candidates);
        let labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
        assert_eq!(labels, ["Virtual environment", "System", "Conda"]);
        assert_eq!(groups[0].toolchains.len(), 2);
        assert_eq!(groups[1].toolchains.len(), 2);
        assert_eq!(picker_groups(&family, &[]), Vec::<PickerGroup>::new());
    }

    #[test]
    fn the_automatic_entry_names_the_best_candidate() {
        let family = family(&[
            ("venv", "Virtual environment"),
            ("conda", "Conda"),
            ("linux-global", "System"),
            ("system", "System"),
        ]);
        assert_eq!(automatic_label(&family, None), "Automatic");
        let best = toolchain("venv", "/a/.venv/bin/python");
        assert_eq!(
            automatic_label(&family, Some(&best)),
            "Automatic (Python (Virtual environment))"
        );
    }
}
