use gpui_kit::component::command::CommandGroup;
use gpui_kit::{AppContext as _, Context};
use krad_domain::ProjectTask;
use krad_repo::detect_tasks;

use super::super::sessions::Launch;
use super::super::EnvironmentShell;
use super::row::{command_row, Glyph};
use crate::components::IconName;

const PALETTE_GROUP_TASKS: &str = "Tasks";
const RUN_TASK_PREFIX: &str = "Run task: ";
const TASK_GLYPH: Glyph = (IconName::Play, None);

pub(super) fn task_label(task: &ProjectTask) -> String {
    format!("{RUN_TASK_PREFIX}{task}")
}

pub(super) fn task_group(tasks: &[ProjectTask]) -> CommandGroup {
    CommandGroup::new().label(PALETTE_GROUP_TASKS).items(
        tasks.iter().map(|task| {
            command_row(task_label(task), TASK_GLYPH, None).keywords([task.name.clone()])
        }),
    )
}

impl EnvironmentShell {
    pub(super) fn detect_palette_tasks(&mut self, cx: &mut Context<Self>) {
        self.palette_tasks.clear();
        let Some(root) = self.palette_root() else {
            return;
        };
        let link = self.active_link();
        let searched = root.clone();
        cx.spawn(async move |this, cx| {
            let detection = cx
                .background_spawn(async move { detect_tasks(&link, &searched) })
                .await;
            for (_, error) in &detection.unreadable {
                tracing::warn!(%error, "project task file unreadable");
            }
            this.update(cx, |shell, cx| {
                if shell.palette_root().as_deref() != Some(root.as_path()) {
                    return;
                }
                shell.palette_tasks = detection.tasks;
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn run_project_task(&mut self, task: ProjectTask, cx: &mut Context<Self>) {
        let label = task.to_string();
        self.spawn_terminal(&label, Launch::Task(task), None, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::cargo_tasks;

    #[test]
    fn a_task_reads_as_the_command_it_runs() {
        let labels: Vec<String> = cargo_tasks().iter().map(task_label).collect();
        assert_eq!(
            labels,
            [
                "Run task: cargo build",
                "Run task: cargo test",
                "Run task: cargo run"
            ]
        );
    }
}
