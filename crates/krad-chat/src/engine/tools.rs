mod edit;
mod fetch;
mod find;
mod grep;
mod mcp_call;
mod parse;
mod read;
#[cfg(test)]
mod scratch;
mod search;
mod shell;
mod specs;
mod write;

use krad_domain::FileEdit;
use serde_json::Value;
use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use super::mcp::{shown_tool, McpToolbox};

pub(crate) use fetch::WebUrl;
pub(crate) use search::Search;
pub(crate) use shell::Background;
pub(crate) use specs::specs;

const READ_TOOL: &str = "read";
const WRITE_TOOL: &str = "write";
const EDIT_TOOL: &str = "edit";
const FIND_TOOL: &str = "find";
const GREP_TOOL: &str = "grep";
const SHELL_TOOL: &str = "shell";
const SHELL_OUTPUT_TOOL: &str = "shell_output";
const SHELL_STOP_TOOL: &str = "shell_stop";
const WEB_FETCH_TOOL: &str = "web_fetch";
const WEB_SEARCH_TOOL: &str = "web_search";
const READ_LABEL: &str = "Read";
const WRITE_LABEL: &str = "Write";
const EDIT_LABEL: &str = "Edit";
const FIND_LABEL: &str = "Glob";
const GREP_LABEL: &str = "Grep";
const SHELL_LABEL: &str = "Bash";
const WEB_FETCH_LABEL: &str = "WebFetch";
const WEB_SEARCH_LABEL: &str = "WebSearch";
const IN_PREFIX: &str = " in ";

#[derive(Clone)]
pub(crate) struct Workspace {
    pub(crate) cwd: PathBuf,
    pub(crate) env: Vec<(String, String)>,
}

impl Workspace {
    pub(crate) fn resolve(&self, path: &Path) -> PathBuf {
        normalize(&self.cwd.join(path))
    }

    pub(crate) fn contains(&self, path: &Path) -> bool {
        self.resolve(path).starts_with(normalize(&self.cwd))
    }

    fn shown(&self, path: &Path) -> String {
        let full = self.resolve(path);
        full.strip_prefix(normalize(&self.cwd))
            .ok()
            .filter(|inside| !inside.as_os_str().is_empty())
            .unwrap_or(&full)
            .to_string_lossy()
            .into_owned()
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other),
        }
    }
    clean
}

pub(crate) struct Context {
    pub(crate) workspace: Workspace,
    pub(crate) background: Background,
    pub(crate) search: Option<Search>,
    pub(crate) mcp: Arc<McpToolbox>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Occurrence {
    Once,
    Every,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Replacement {
    pub(crate) old: String,
    pub(crate) new: String,
    pub(crate) occurrence: Occurrence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellRun {
    Wait,
    Background,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Read {
        path: PathBuf,
        offset: Option<usize>,
        limit: Option<usize>,
    },
    Write {
        path: PathBuf,
        content: String,
    },
    Edit {
        path: PathBuf,
        replacements: Vec<Replacement>,
    },
    Find {
        pattern: String,
        path: Option<PathBuf>,
    },
    Grep {
        pattern: String,
        path: Option<PathBuf>,
        glob: Option<String>,
        ignore_case: bool,
    },
    Shell {
        command: String,
        timeout: Option<Duration>,
        run: ShellRun,
    },
    ShellOutput {
        id: String,
    },
    ShellStop {
        id: String,
    },
    WebFetch {
        url: WebUrl,
        prompt: Option<String>,
    },
    WebSearch {
        query: String,
    },
    Mcp {
        name: String,
        arguments: Value,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Ran {
    pub(crate) content: String,
    pub(crate) failed: bool,
    pub(crate) edit: Option<FileEdit>,
}

impl Ran {
    fn done(content: String) -> Self {
        Self {
            content,
            ..Self::default()
        }
    }

    fn failed(content: String) -> Self {
        Self {
            content,
            failed: true,
            edit: None,
        }
    }
}

impl Action {
    pub(crate) fn label(&self) -> Cow<'static, str> {
        let label = match self {
            Self::Read { .. } => READ_LABEL,
            Self::Write { .. } => WRITE_LABEL,
            Self::Edit { .. } => EDIT_LABEL,
            Self::Find { .. } => FIND_LABEL,
            Self::Grep { .. } => GREP_LABEL,
            Self::Shell { .. } | Self::ShellOutput { .. } | Self::ShellStop { .. } => SHELL_LABEL,
            Self::WebFetch { .. } => WEB_FETCH_LABEL,
            Self::WebSearch { .. } => WEB_SEARCH_LABEL,
            Self::Mcp { name, .. } => return Cow::Owned(shown_tool(name)),
        };
        Cow::Borrowed(label)
    }

    pub(crate) fn summary(&self, workspace: &Workspace) -> String {
        let searched = |pattern: &str, path: Option<&PathBuf>| match path {
            Some(path) => format!("{pattern}{IN_PREFIX}{}", workspace.shown(path)),
            None => pattern.to_owned(),
        };
        match self {
            Self::Read { path, .. } | Self::Write { path, .. } | Self::Edit { path, .. } => {
                workspace.shown(path)
            }
            Self::Find { pattern, path } | Self::Grep { pattern, path, .. } => {
                searched(pattern, path.as_ref())
            }
            Self::Shell { command, .. } => command.clone(),
            Self::ShellOutput { id } | Self::ShellStop { id } => id.clone(),
            Self::WebFetch { url, .. } => url.to_string(),
            Self::WebSearch { query } => query.clone(),
            Self::Mcp { arguments, .. } => mcp_call::summary(arguments),
        }
    }

    pub(crate) fn preview(&self, workspace: &Workspace) -> Option<FileEdit> {
        match self {
            Self::Write { path, content } => Some(write::preview(
                &workspace.resolve(path),
                &workspace.shown(path),
                content,
            )),
            Self::Edit { path, replacements } => Some(FileEdit {
                path: workspace.shown(path),
                lines: edit::diff(replacements),
            }),
            Self::Read { .. }
            | Self::Find { .. }
            | Self::Grep { .. }
            | Self::Shell { .. }
            | Self::ShellOutput { .. }
            | Self::ShellStop { .. }
            | Self::WebFetch { .. }
            | Self::WebSearch { .. }
            | Self::Mcp { .. } => None,
        }
    }

    pub(crate) fn resolve(&self, workspace: &Workspace) -> Option<PathBuf> {
        match self {
            Self::Read { path, .. } | Self::Write { path, .. } | Self::Edit { path, .. } => {
                Some(workspace.resolve(path))
            }
            Self::Find { path, .. } | Self::Grep { path, .. } => Some(
                path.as_deref()
                    .map_or_else(|| normalize(&workspace.cwd), |path| workspace.resolve(path)),
            ),
            Self::Shell { .. }
            | Self::ShellOutput { .. }
            | Self::ShellStop { .. }
            | Self::WebFetch { .. }
            | Self::WebSearch { .. }
            | Self::Mcp { .. } => None,
        }
    }

    pub(crate) fn written(&self, workspace: &Workspace) -> Option<PathBuf> {
        match self {
            Self::Write { path, .. } | Self::Edit { path, .. } => Some(workspace.resolve(path)),
            _ => None,
        }
    }

    pub(crate) fn run(&self, context: &Context, cancel: &AtomicBool) -> Ran {
        let workspace = &context.workspace;
        let root = self.resolve(workspace).unwrap_or_default();
        match self {
            Self::Read {
                path,
                offset,
                limit,
            } => read::read(&root, &workspace.shown(path), *offset, *limit),
            Self::Write { path, content } => write::write(&root, &workspace.shown(path), content),
            Self::Edit { path, replacements } => {
                edit::edit(&root, &workspace.shown(path), replacements)
            }
            Self::Find { pattern, .. } => find::find(workspace, &root, pattern, cancel),
            Self::Grep {
                pattern,
                glob,
                ignore_case,
                ..
            } => grep::grep(
                workspace,
                &root,
                &grep::Query {
                    pattern,
                    glob: glob.as_deref(),
                    ignore_case: *ignore_case,
                },
                cancel,
            ),
            Self::Shell {
                command,
                timeout,
                run: ShellRun::Wait,
            } => shell::run(workspace, command, *timeout, cancel),
            Self::Shell {
                command,
                run: ShellRun::Background,
                ..
            } => context.background.start(workspace, command),
            Self::ShellOutput { id } => context.background.output(id),
            Self::ShellStop { id } => context.background.stop(id),
            Self::WebFetch { url, .. } => fetch::fetch(url, cancel),
            Self::WebSearch { query } => search::search(context.search.as_ref(), query, cancel),
            Self::Mcp { name, arguments } => mcp_call::run(&context.mcp, name, arguments),
        }
    }
}

#[cfg(test)]
mod tests;
