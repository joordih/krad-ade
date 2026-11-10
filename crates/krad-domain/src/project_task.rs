use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{quote_for_shell, ShellQuoting};

mod cargo;
mod just;
mod make;
mod package;

pub use cargo::cargo_tasks;
pub use just::just_tasks;
pub use make::make_tasks;
pub use package::{package_script_tasks, NodePackageManager, PACKAGE_LOCKFILES};

pub const PACKAGE_MANIFEST: &str = "package.json";
pub const CARGO_MANIFEST: &str = "Cargo.toml";
pub const JUSTFILE_NAMES: [&str; 3] = ["justfile", "Justfile", ".justfile"];
pub const MAKEFILE_NAMES: [&str; 3] = ["GNUmakefile", "makefile", "Makefile"];

const ARG_SEPARATOR: &str = " ";
const PLAIN_PUNCTUATION: [char; 7] = ['-', '_', '.', '/', ':', '=', '+'];
const OPTION_PREFIX: char = '-';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskSource {
    PackageScripts(NodePackageManager),
    Cargo,
    Just,
    Make,
}

impl TaskSource {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PackageScripts(manager) => manager.program(),
            Self::Cargo => "cargo",
            Self::Just => "just",
            Self::Make => "make",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectTask {
    pub source: TaskSource,
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
}

impl ProjectTask {
    fn new(source: TaskSource, name: &str, program: &str, args: &[&str]) -> Self {
        Self {
            source,
            name: name.to_owned(),
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }

    #[must_use]
    pub fn command_line(&self, quoting: ShellQuoting) -> String {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().map(|arg| shell_word(arg, quoting)))
            .collect::<Vec<_>>()
            .join(ARG_SEPARATOR)
    }
}

impl fmt::Display for ProjectTask {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.program)?;
        for arg in &self.args {
            write!(formatter, "{ARG_SEPARATOR}{arg}")?;
        }
        Ok(())
    }
}

fn shell_word(arg: &str, quoting: ShellQuoting) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || PLAIN_PUNCTUATION.contains(&ch));
    if plain {
        arg.to_owned()
    } else {
        quote_for_shell(arg, quoting)
    }
}

fn is_task_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(OPTION_PREFIX)
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_words_stay_bare_and_others_are_quoted_for_the_shell() {
        let task = ProjectTask::new(
            TaskSource::PackageScripts(NodePackageManager::Npm),
            "lint fix",
            "npm",
            &["run", "lint fix"],
        );
        assert_eq!(task.command_line(ShellQuoting::Posix), "npm run 'lint fix'");
        assert_eq!(task.to_string(), "npm run lint fix");
        let plain = ProjectTask::new(TaskSource::Cargo, "test", "cargo", &["test"]);
        assert_eq!(plain.command_line(ShellQuoting::PowerShell), "cargo test");
    }

    #[test]
    fn a_task_name_is_a_plain_word_that_is_never_read_as_an_option() {
        assert!(is_task_name("build-all"));
        assert!(is_task_name("check_fmt"));
        for name in ["", "-j", "--always-make", "two words", "a.b"] {
            assert!(!is_task_name(name), "{name}");
        }
    }
}
