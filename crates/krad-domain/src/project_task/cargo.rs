use super::{ProjectTask, TaskSource};

const CARGO_PROGRAM: &str = "cargo";
const CARGO_COMMANDS: [&str; 3] = ["build", "test", "run"];

#[must_use]
pub fn cargo_tasks() -> Vec<ProjectTask> {
    CARGO_COMMANDS
        .iter()
        .map(|command| ProjectTask::new(TaskSource::Cargo, command, CARGO_PROGRAM, &[command]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cargo_project_builds_tests_and_runs() {
        let lines: Vec<String> = cargo_tasks().iter().map(ToString::to_string).collect();
        assert_eq!(lines, ["cargo build", "cargo test", "cargo run"]);
    }
}
