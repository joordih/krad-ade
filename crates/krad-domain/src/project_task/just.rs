use super::{is_task_name, ProjectTask, TaskSource};

const JUST_PROGRAM: &str = "just";
const COMMENT: char = '#';
const ATTRIBUTE_OPEN: char = '[';
const PRIVATE_ATTRIBUTE: &str = "private";
const PRIVATE_PREFIX: char = '_';
const QUIET_PREFIX: char = '@';
const RECIPE_END: char = ':';
const ASSIGNMENT: &str = ":=";
const NOT_RECIPES: [&str; 7] = ["set", "alias", "export", "import", "mod", "if", "unexport"];

#[must_use]
pub fn just_tasks(justfile: &str) -> Vec<ProjectTask> {
    let mut tasks: Vec<ProjectTask> = Vec::new();
    let mut private = false;
    for line in justfile.lines() {
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let line = line.trim_end();
        if line.is_empty() || line.starts_with(COMMENT) {
            continue;
        }
        if line.starts_with(ATTRIBUTE_OPEN) {
            private |= line.contains(PRIVATE_ATTRIBUTE);
            continue;
        }
        let hidden = std::mem::take(&mut private);
        let Some(name) = recipe_name(line) else {
            continue;
        };
        if hidden || name.starts_with(PRIVATE_PREFIX) || tasks.iter().any(|task| task.name == name)
        {
            continue;
        }
        tasks.push(ProjectTask::new(
            TaskSource::Just,
            name,
            JUST_PROGRAM,
            &[name],
        ));
    }
    tasks
}

fn recipe_name(line: &str) -> Option<&str> {
    if line.contains(ASSIGNMENT) {
        return None;
    }
    let (head, _) = line.split_once(RECIPE_END)?;
    let mut words = head.split_whitespace();
    let name = words.next()?.trim_start_matches(QUIET_PREFIX);
    if NOT_RECIPES.contains(&name) || !is_task_name(name) {
        return None;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JUSTFILE: &str = "\
set shell := [\"bash\", \"-c\"]
version := `git describe`
alias b := build

# Build everything
build:
    cargo build

@test filter='': build
    cargo test {{filter}}

[private]
helper:
    echo hidden

_internal:
    echo hidden

[group('ci')]
lint:
    cargo clippy
";

    #[test]
    fn public_recipes_are_tasks_and_settings_aliases_and_private_ones_are_not() {
        let names: Vec<String> = just_tasks(JUSTFILE)
            .into_iter()
            .map(|task| task.to_string())
            .collect();
        assert_eq!(names, ["just build", "just test", "just lint"]);
    }
}
