use futures::channel::oneshot;
use krad_agents::Response;
use krad_domain::{
    CleanupPlan, DiffSummary, EnvironmentName, GroupId, ProjectTask, TestOutcome, VariantId,
    VariantRemoval, VariantRunId, VariantSpec, Worktree,
};
use std::path::PathBuf;

const UNKNOWN_TASK: &str = "The group has no project task named";
const TASK_LIST_SEPARATOR: &str = ", ";
const KNOWN_TASKS: &str = "; known tasks: ";

#[derive(Clone, Debug)]
pub(crate) struct VariantLaunch {
    pub(crate) environment: EnvironmentName,
    pub(crate) repo_root: PathBuf,
    pub(crate) run: VariantRunId,
    pub(crate) variant: VariantId,
    pub(crate) name: String,
}

#[derive(Debug)]
pub(crate) struct PickAsked {
    pub(crate) environment: EnvironmentName,
    pub(crate) plan: CleanupPlan,
    pub(crate) removal: VariantRemoval,
    pub(crate) reply: Option<oneshot::Sender<Response>>,
}

#[derive(Debug)]
pub(crate) struct BusOrder {
    pub(crate) environment: EnvironmentName,
    pub(crate) group: GroupId,
    pub(crate) prompt: String,
    pub(crate) variants: Vec<VariantSpec>,
    pub(crate) test: String,
    pub(crate) reply: oneshot::Sender<Response>,
}

#[derive(Debug)]
pub(crate) enum VariantJob {
    WorktreeAdded {
        launch: VariantLaunch,
        created: Result<Worktree, String>,
    },
    Summarized {
        environment: EnvironmentName,
        variant: (VariantRunId, VariantId),
        summary: Result<DiffSummary, String>,
    },
    Tested {
        environment: EnvironmentName,
        variant: (VariantRunId, VariantId),
        outcome: TestOutcome,
    },
    Applied {
        pick: PickAsked,
        applied: Result<(), String>,
    },
    Removed {
        environment: EnvironmentName,
        errors: Vec<String>,
    },
    Checked {
        environment: EnvironmentName,
        gone: Vec<PathBuf>,
    },
    TasksRead {
        order: BusOrder,
        tasks: Vec<ProjectTask>,
    },
}

pub(crate) fn chosen_task(tasks: Vec<ProjectTask>, named: &str) -> Result<ProjectTask, String> {
    let wanted = named.trim();
    let known: Vec<String> = tasks.iter().map(ToString::to_string).collect();
    tasks
        .into_iter()
        .find(|task| task.to_string() == wanted || task.name == wanted)
        .ok_or_else(|| {
            format!(
                "{UNKNOWN_TASK} `{wanted}`{KNOWN_TASKS}{}",
                known.join(TASK_LIST_SEPARATOR)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::TaskSource;

    fn task(name: &str) -> ProjectTask {
        ProjectTask {
            source: TaskSource::Cargo,
            name: name.to_owned(),
            program: "cargo".to_owned(),
            args: vec![name.to_owned()],
        }
    }

    #[test]
    fn the_test_task_is_found_by_its_command_or_its_name() {
        let tasks = || vec![task("build"), task("test")];
        assert_eq!(chosen_task(tasks(), "cargo test"), Ok(task("test")));
        assert_eq!(chosen_task(tasks(), " test "), Ok(task("test")));
        assert_eq!(
            chosen_task(tasks(), "npm test"),
            Err("The group has no project task named `npm test`; known tasks: cargo build, cargo test".to_owned())
        );
    }
}
