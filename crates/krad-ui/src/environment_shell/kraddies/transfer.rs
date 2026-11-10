use gpui_kit::{Context, PathPromptOptions, SharedString};
use krad_domain::{Automation, KraddieHistory, KraddieId, KRADDIE_FILE_EXTENSION};
use krad_store::{export_kraddie, import_kraddie};
use std::path::PathBuf;

use super::super::EnvironmentShell;
use super::panel::NAME_TAKEN;
use super::AutomationEntry;

const IMPORT_PROMPT: &str = "Import Kraddie";
const IMPORTED: &str = "Kraddie imported";
const EXPORTED: &str = "Kraddie exported";

impl EnvironmentShell {
    pub(super) fn export_kraddie_file(&mut self, id: &KraddieId, cx: &mut Context<Self>) {
        let Some((kraddie, automations)) = self.kraddies.entry(id).map(|entry| {
            let automations: Vec<Automation> = entry
                .automations
                .iter()
                .map(|known| known.automation.clone())
                .collect();
            (entry.kraddie.clone(), automations)
        }) else {
            return;
        };
        let suggested = PathBuf::from(id.as_str()).with_extension(KRADDIE_FILE_EXTENSION);
        let folder = dirs_home();
        let answer = cx.prompt_for_new_path(&folder, suggested.to_str());
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = answer.await else {
                return;
            };
            let written = cx
                .background_executor()
                .spawn(async move { export_kraddie(&kraddie, &automations, &path) })
                .await;
            this.update(cx, |shell, cx| match written {
                Ok(()) => shell.toast(EXPORTED, cx),
                Err(error) => shell.toast(&error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn import_kraddie_file(cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(IMPORT_PROMPT)),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = answer.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let read = cx
                .background_executor()
                .spawn(async move { import_kraddie(&path) })
                .await;
            this.update(cx, |shell, cx| match read {
                Ok(read) if shell.kraddies.entry(&read.kraddie.id()).is_some() => {
                    shell.toast(NAME_TAKEN, cx);
                }
                Ok(read) => {
                    let automations = read
                        .automations
                        .into_iter()
                        .map(|automation| AutomationEntry {
                            automation,
                            history: KraddieHistory::default(),
                        })
                        .collect();
                    shell.adopt_new_kraddie(read.kraddie, automations, None, cx);
                    shell.toast(IMPORTED, cx);
                }
                Err(error) => shell.toast(&error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

fn dirs_home() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}
