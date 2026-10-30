use std::fs;
use std::path::{Path, PathBuf};

use crate::prompt::shared_skill_message;
use crate::skills::SkillFile;

const BRIEF: &str = "You are a coding agent working inside Krad ADE, a desktop app where the user runs many agents in sessions. You help with software engineering tasks in the user's workspace by reading code, editing files and running commands with your tools.

Guidelines:
- Read the relevant files before changing them; prefer edit for changes to existing files and write only for new files or full rewrites.
- Use find and grep to explore instead of guessing paths.
- Use shell for builds, tests, git and other commands; keep commands non-interactive. Start servers and watchers with run_in_background, read them with shell_output and stop them with shell_stop.
- Use web_search for current information and web_fetch to read a page.
- Make the smallest change that solves the task and follow the conventions of the code around it.
- When a choice is the user's to make, ask with ask_user instead of guessing.
- For tasks with three or more steps, keep a checklist with the checklist tool: write the steps when you start and update it as you go, with exactly one item in_progress.
- Hand broad searches and independent pieces of work to subagents with the agent tool; several agent calls in one reply run in parallel.
- In Plan mode, explore read-only and finish by calling exit_plan with the plan.
- When a tool call is denied, do not retry it as is; adjust or ask the user.
- Answer concisely in Markdown. Show file paths relative to the workspace.";
const ENVIRONMENT_HEADING: &str = "Environment";
const WORKSPACE_LABEL: &str = "Workspace";
const PLATFORM_LABEL: &str = "Platform";
const INSTRUCTIONS_HEADING: &str = "Project instructions";
const SKILLS_HEADING: &str = "Skills";
const SKILLS_INTRO: &str = "Skills are instructions for specific tasks. When a task matches a skill, read its file with the read tool first and follow it.";
const SECTION_GAP: &str = "\n\n";
const HEADING_MARK: &str = "## ";
const SOURCE_MARK: &str = "### ";
const INSTRUCTION_FILES: [&str; 2] = ["AGENTS.md", "CLAUDE.md"];
const HOME_INSTRUCTION_DIRS: [&str; 2] = [".agents", ".claude"];
const GIT_DIR: &str = ".git";
const SKILL_MARK: char = '/';

fn ancestors_to_root(cwd: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in cwd.ancestors() {
        found.push(dir.to_path_buf());
        if dir.join(GIT_DIR).exists() {
            break;
        }
    }
    found.reverse();
    found
}

fn first_instructions(dir: &Path) -> Option<(PathBuf, String)> {
    INSTRUCTION_FILES.iter().find_map(|name| {
        let path = dir.join(name);
        let text = fs::read_to_string(&path).ok()?;
        (!text.trim().is_empty()).then_some((path, text))
    })
}

pub(crate) fn instruction_files(home: Option<&Path>, cwd: &Path) -> Vec<(PathBuf, String)> {
    let home_files = home.into_iter().flat_map(|home| {
        HOME_INSTRUCTION_DIRS
            .iter()
            .filter_map(move |dir| first_instructions(&home.join(dir)))
            .take(1)
    });
    let project_files = ancestors_to_root(cwd)
        .into_iter()
        .filter(|dir| Some(dir.as_path()) != home)
        .filter_map(|dir| first_instructions(&dir));
    home_files.chain(project_files).collect()
}

pub(crate) struct PromptParts<'a> {
    pub(crate) cwd: &'a Path,
    pub(crate) instructions: &'a [(PathBuf, String)],
    pub(crate) skills: &'a [SkillFile],
    pub(crate) extra: Option<&'a str>,
}

pub(crate) fn system_prompt(parts: &PromptParts<'_>) -> String {
    let mut sections = vec![
        BRIEF.to_owned(),
        format!(
            "{HEADING_MARK}{ENVIRONMENT_HEADING}\n{WORKSPACE_LABEL}: {}\n{PLATFORM_LABEL}: {}",
            parts.cwd.display(),
            std::env::consts::OS
        ),
    ];
    if !parts.instructions.is_empty() {
        sections.push(format!("{HEADING_MARK}{INSTRUCTIONS_HEADING}"));
        sections.extend(
            parts
                .instructions
                .iter()
                .map(|(path, text)| format!("{SOURCE_MARK}{}\n{}", path.display(), text.trim())),
        );
    }
    if !parts.skills.is_empty() {
        let listed: Vec<String> = parts
            .skills
            .iter()
            .map(|skill| {
                format!(
                    "- {}: {} ({})",
                    skill.name,
                    skill.description,
                    skill.file.display()
                )
            })
            .collect();
        sections.push(format!(
            "{HEADING_MARK}{SKILLS_HEADING}\n{SKILLS_INTRO}\n{}",
            listed.join("\n")
        ));
    }
    if let Some(extra) = parts.extra.map(str::trim).filter(|extra| !extra.is_empty()) {
        sections.push(extra.to_owned());
    }
    sections.join(SECTION_GAP)
}

pub(crate) fn expand_skill_call(text: &str, skills: &[SkillFile]) -> Option<String> {
    let name = text
        .trim_start()
        .strip_prefix(SKILL_MARK)?
        .split_whitespace()
        .next()?;
    let skill = skills.iter().find(|skill| skill.name == name)?;
    Some(shared_skill_message(text, &skill.file.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::new_session_id;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir()
            .join("krad-engine-prompt")
            .join(new_session_id().as_str());
        fs::create_dir_all(&dir).unwrap_or_default();
        dir
    }

    #[test]
    fn instructions_come_from_home_then_the_repository_down_to_the_workspace() {
        let home = scratch();
        let repo = home.join("repo");
        let crate_dir = repo.join("crates").join("app");
        fs::create_dir_all(home.join(".claude")).unwrap_or_default();
        fs::create_dir_all(repo.join(GIT_DIR)).unwrap_or_default();
        fs::create_dir_all(&crate_dir).unwrap_or_default();
        fs::write(home.join(".claude").join("CLAUDE.md"), "home rules").unwrap_or_default();
        fs::write(repo.join("AGENTS.md"), "repo rules").unwrap_or_default();
        fs::write(repo.join("CLAUDE.md"), "ignored, AGENTS.md wins").unwrap_or_default();
        fs::write(crate_dir.join("CLAUDE.md"), "crate rules").unwrap_or_default();
        let texts: Vec<String> = instruction_files(Some(&home), &crate_dir)
            .into_iter()
            .map(|(_, text)| text)
            .collect();
        assert_eq!(texts, ["home rules", "repo rules", "crate rules"]);
    }

    #[test]
    fn the_prompt_lists_the_workspace_skills_and_the_extra_guide() {
        let skills = [SkillFile {
            name: "release".to_owned(),
            description: "Cut a release".to_owned(),
            file: PathBuf::from("/s/release/SKILL.md"),
        }];
        let prompt = system_prompt(&PromptParts {
            cwd: Path::new("/work"),
            instructions: &[(PathBuf::from("/work/AGENTS.md"), "Be brief.".to_owned())],
            skills: &skills,
            extra: Some("Use the bus."),
        });
        assert!(prompt.contains("Workspace: /work"));
        assert!(prompt.contains("Be brief."));
        assert!(prompt.contains("- release: Cut a release (/s/release/SKILL.md)"));
        assert!(prompt.ends_with("Use the bus."));
    }

    #[test]
    fn a_skill_call_becomes_an_order_to_read_its_file() {
        let skills = [SkillFile {
            name: "release".to_owned(),
            description: String::new(),
            file: PathBuf::from("/s/release/SKILL.md"),
        }];
        let expanded = expand_skill_call("/release v2", &skills).unwrap_or_default();
        assert!(expanded.contains("/s/release/SKILL.md"));
        assert!(expanded.ends_with("v2"));
        assert_eq!(expand_skill_call("/unknown", &skills), None);
        assert_eq!(expand_skill_call("plain text", &skills), None);
    }
}
