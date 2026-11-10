use super::{is_task_name, ProjectTask, TaskSource};

const MAKE_PROGRAM: &str = "make";
const COMMENT: char = '#';
const RULE_END: char = ':';
const ASSIGNMENT_MARKS: [&str; 5] = [":=", "::=", "?=", "+=", "!="];
const PLAIN_ASSIGNMENT: char = '=';

#[must_use]
pub fn make_tasks(makefile: &str) -> Vec<ProjectTask> {
    let mut tasks: Vec<ProjectTask> = Vec::new();
    for line in makefile.lines() {
        if line.starts_with(char::is_whitespace) || line.starts_with(COMMENT) {
            continue;
        }
        for name in rule_targets(line) {
            if tasks.iter().any(|task| task.name == name) {
                continue;
            }
            tasks.push(ProjectTask::new(
                TaskSource::Make,
                name,
                MAKE_PROGRAM,
                &[name],
            ));
        }
    }
    tasks
}

fn rule_targets(line: &str) -> impl Iterator<Item = &str> {
    let assigns = ASSIGNMENT_MARKS.iter().any(|mark| line.contains(mark));
    let targets = line
        .split_once(RULE_END)
        .filter(|(head, _)| !assigns && !head.contains(PLAIN_ASSIGNMENT))
        .map_or("", |(head, _)| head);
    targets.split_whitespace().filter(|name| is_task_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAKEFILE: &str = "\
CC ?= cc
FLAGS := -O2
PREFIX = /usr/local
.PHONY: all clean test

# The default target
all: build/app

build/app: main.o util.o
\t$(CC) -o $@ $^

%.o: %.c
\t$(CC) -c $<

clean test:
\trm -rf build
";

    #[test]
    fn named_rules_are_tasks_and_files_patterns_and_variables_are_not() {
        let names: Vec<String> = make_tasks(MAKEFILE)
            .into_iter()
            .map(|task| task.to_string())
            .collect();
        assert_eq!(names, ["make all", "make clean", "make test"]);
    }
}
