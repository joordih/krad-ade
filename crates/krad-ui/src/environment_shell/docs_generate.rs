mod sheet;

use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, Entity, Window};
use krad_domain::{docs_prompt, AgentPreset, ModelId, DEFAULT_DOCS_FOLDER, VAULT_DIR};
use krad_repo::{docs_source_candidates, read_docs_sources, write_docs_sources, SourceEntry};

use super::sessions::Launch;
use super::EnvironmentShell;

const NOTE_CHOICES: [(&str, &str); 6] = [
    (
        "Overview",
        "What the project is and how its parts fit together.",
    ),
    (
        "Architecture",
        "Its modules, their responsibilities and how they depend on each other.",
    ),
    (
        "Glossary",
        "The terms the code and the team use, and what each means.",
    ),
    (
        "Decisions",
        "The choices made so far and the reasons behind them.",
    ),
    (
        "How-to guides",
        "Step-by-step tasks: building, testing, releasing.",
    ),
    ("API reference", "The public items each part exposes."),
];
const DEFAULT_NOTES: usize = 4;
const TERMINAL_LABEL: &str = "docs";
const STARTED_TEXT: &str = "Writing the documentation in a new terminal";
const FOLDER_PLACEHOLDER: &str = "Vault folder, empty for the vault's root";

pub(super) struct DocsDialog {
    sources: Vec<(SourceEntry, bool)>,
    notes: Vec<(&'static str, bool)>,
    folder: Entity<InputState>,
    keep_edited: bool,
    preset: Option<AgentPreset>,
    model: Option<ModelId>,
    picker_open: bool,
}

impl EnvironmentShell {
    pub(super) fn open_docs_generate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.active_group().map(|group| group.root.clone()) else {
            return;
        };
        let (preset, model) = self.default_agent_choice();
        let picked = read_docs_sources(&root).unwrap_or_default();
        let sources = docs_source_candidates(&root)
            .unwrap_or_default()
            .into_iter()
            .map(|entry| {
                let chosen = picked
                    .as_ref()
                    .is_none_or(|picked| picked.contains(&entry.relative));
                (entry, chosen)
            })
            .collect();
        let folder = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(FOLDER_PLACEHOLDER)
                .default_value(DEFAULT_DOCS_FOLDER)
        });
        self.docs_dialog = Some(DocsDialog {
            sources,
            notes: NOTE_CHOICES
                .iter()
                .enumerate()
                .map(|(index, (note, _))| (*note, index < DEFAULT_NOTES))
                .collect(),
            folder,
            keep_edited: true,
            preset,
            model,
            picker_open: false,
        });
        cx.notify();
    }

    fn start_docs_generation(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.docs_dialog.take() else {
            return;
        };
        let Some(preset) = dialog.preset else {
            return;
        };
        let notes: Vec<String> = dialog
            .notes
            .iter()
            .filter(|(_, chosen)| *chosen)
            .map(|(note, _)| (*note).to_owned())
            .collect();
        let sources: Vec<String> = dialog
            .sources
            .iter()
            .filter(|(_, chosen)| *chosen)
            .map(|(entry, _)| entry.relative.clone())
            .collect();
        let keep: Vec<String> = if dialog.keep_edited {
            self.notes
                .index
                .notes()
                .iter()
                .filter(|note| note.generated && note.edited)
                .map(|note| format!("{VAULT_DIR}/{}", note.path))
                .collect()
        } else {
            Vec::new()
        };
        let folder = dialog.folder.read(cx).value().to_string();
        let prompt = docs_prompt(
            &notes,
            &folder,
            &sources,
            &keep,
            preset.display_name(),
            dialog.model.as_ref(),
        );
        if let Some(root) = self.active_group().map(|group| group.root.clone()) {
            let saved = sources.clone();
            let write = cx.background_spawn(async move { write_docs_sources(&root, &saved) });
            cx.spawn(async move |this, cx| {
                if let Err(error) = write.await {
                    this.update(cx, |shell, _| shell.push_toast(error.to_string()))
                        .unwrap_or_default();
                }
            })
            .detach();
        }
        let env = self.active.clone();
        let group = self.active_group().map(|group| group.id);
        self.spawn_terminal_in(
            &env,
            group,
            TERMINAL_LABEL,
            Launch::Agent {
                preset,
                prompt,
                model: dialog.model,
            },
            None,
            cx,
        );
        self.push_toast(STARTED_TEXT.to_owned());
        cx.notify();
    }
}
