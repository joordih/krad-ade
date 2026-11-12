use krad_domain::ResumeId;
use krad_host::{HostError, HostLink};
use std::path::{Path, PathBuf};

use crate::history::{claude_conversation_file, CONVERSATION_EXTENSION, PROJECTS_DIR};
use crate::skills::CLAUDE_DIR;

const FOLDER_SEPARATOR: char = '-';
const CWD_KEY: &str = "\"cwd\":";

#[derive(Debug, thiserror::Error)]
pub enum ConversationCopyError {
    #[error("The conversation's transcript was not found")]
    NotFound,
    #[error("The conversation's transcript could not be copied: {0}")]
    Host(#[from] HostError),
}

pub struct ConversationMove<'a> {
    pub from: &'a HostLink,
    pub from_home: &'a Path,
    pub from_cwd: Option<&'a Path>,
    pub to: &'a HostLink,
    pub to_home: &'a Path,
    pub to_cwd: &'a Path,
}

fn project_folder(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                FOLDER_SEPARATOR
            }
        })
        .collect()
}

#[must_use]
pub fn claude_project_dir(home: &Path, cwd: &Path) -> PathBuf {
    home.join(CLAUDE_DIR)
        .join(PROJECTS_DIR)
        .join(project_folder(cwd))
}

fn quoted(path: &Path) -> String {
    serde_json::Value::String(path.to_string_lossy().into_owned()).to_string()
}

fn moved_folder(text: &str, from: Option<&Path>, to: &Path) -> String {
    let Some(from) = from else {
        return text.to_owned();
    };
    text.replace(
        &format!("{CWD_KEY}{}", quoted(from)),
        &format!("{CWD_KEY}{}", quoted(to)),
    )
}

/// # Errors
///
/// [`ConversationCopyError::NotFound`] when the source has no transcript for
/// `id`, and [`ConversationCopyError::Host`] when it cannot be read or the
/// copy cannot be written.
#[tracing::instrument(skip_all, fields(conversation = id.as_str()))]
pub fn copy_claude_conversation(
    route: &ConversationMove<'_>,
    id: &ResumeId,
) -> Result<PathBuf, ConversationCopyError> {
    let source = claude_conversation_file(route.from, route.from_home, id)
        .ok_or(ConversationCopyError::NotFound)?;
    let bytes = route.from.read_range(&source, 0, u64::MAX)?;
    let text = moved_folder(
        &String::from_utf8_lossy(&bytes),
        route.from_cwd,
        route.to_cwd,
    );
    let folder = claude_project_dir(route.to_home, route.to_cwd);
    route.to.create_dir_all(&folder)?;
    let target = folder
        .join(id.as_str())
        .with_extension(CONVERSATION_EXTENSION);
    route.to.write(&target, text.as_bytes())?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_named_as_claude_code_names_it() {
        assert_eq!(
            claude_project_dir(
                Path::new("/home/me"),
                Path::new("/home/me/.krad-ade/runner/worktrees/api_1/api-a1")
            ),
            Path::new("/home/me/.claude/projects/-home-me--krad-ade-runner-worktrees-api-1-api-a1")
        );
    }

    #[test]
    fn the_copied_lines_point_at_the_new_folder() {
        let line = r#"{"type":"user","cwd":"/Users/me/app","message":"in /Users/me/app"}"#;
        let moved = moved_folder(line, Some(Path::new("/Users/me/app")), Path::new("/r/w"));
        assert_eq!(
            moved,
            r#"{"type":"user","cwd":"/r/w","message":"in /Users/me/app"}"#
        );
        assert_eq!(moved_folder(line, None, Path::new("/r/w")), line);
    }

    #[test]
    fn a_missing_conversation_is_reported_as_such() -> Result<(), Box<dyn std::error::Error>> {
        let home = std::env::temp_dir().join(format!("krad-chat-copy-{}", std::process::id()));
        let target = home.join("target");
        let id: ResumeId = "0f7a9a2e-1c1b-4c55-9d7e-6b1a4f0c2d11".parse()?;
        let route = ConversationMove {
            from: &HostLink::Local,
            from_home: &home,
            from_cwd: None,
            to: &HostLink::Local,
            to_home: &target,
            to_cwd: Path::new("/w"),
        };
        assert!(matches!(
            copy_claude_conversation(&route, &id),
            Err(ConversationCopyError::NotFound)
        ));
        let folder = home.join(".claude/projects/-src-app");
        std::fs::create_dir_all(&folder)?;
        std::fs::write(
            folder.join(format!("{}.jsonl", id.as_str())),
            "{\"cwd\":\"/src/app\"}\n",
        )?;
        let moved = ConversationMove {
            from_cwd: Some(Path::new("/src/app")),
            ..route
        };
        let written = copy_claude_conversation(&moved, &id)?;
        assert_eq!(std::fs::read_to_string(&written)?, "{\"cwd\":\"/w\"}\n");
        std::fs::remove_dir_all(&home)?;
        Ok(())
    }
}
