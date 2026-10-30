use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

use crate::engine::mcp::is_mcp_tool;

use super::{
    Action, Occurrence, Replacement, ShellRun, WebUrl, EDIT_TOOL, FIND_TOOL, GREP_TOOL, READ_TOOL,
    SHELL_OUTPUT_TOOL, SHELL_STOP_TOOL, SHELL_TOOL, WEB_FETCH_TOOL, WEB_SEARCH_TOOL, WRITE_TOOL,
};

pub(super) const UNKNOWN_TOOL_TEXT: &str = "Unknown tool";
pub(super) const BAD_ARGUMENTS_TEXT: &str = "Invalid arguments for";
const NO_EDITS_TEXT: &str = "give old_string and new_string, or a non-empty edits list";
const EMPTY_QUERY_TEXT: &str = "the query is empty";

#[derive(Deserialize)]
struct ReadArgs {
    path: PathBuf,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct WriteArgs {
    path: PathBuf,
    content: String,
}

#[derive(Deserialize)]
struct ReplacementArgs {
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

impl From<ReplacementArgs> for Replacement {
    fn from(args: ReplacementArgs) -> Self {
        Self {
            old: args.old_string,
            new: args.new_string,
            occurrence: if args.replace_all {
                Occurrence::Every
            } else {
                Occurrence::Once
            },
        }
    }
}

#[derive(Deserialize)]
struct EditArgs {
    path: PathBuf,
    old_string: Option<String>,
    new_string: Option<String>,
    #[serde(default)]
    replace_all: bool,
    #[serde(default)]
    edits: Vec<ReplacementArgs>,
}

impl EditArgs {
    fn into_action(self) -> Option<Action> {
        let replacements = if self.edits.is_empty() {
            vec![ReplacementArgs {
                old_string: self.old_string?,
                new_string: self.new_string?,
                replace_all: self.replace_all,
            }
            .into()]
        } else {
            self.edits.into_iter().map(Replacement::from).collect()
        };
        Some(Action::Edit {
            path: self.path,
            replacements,
        })
    }
}

#[derive(Deserialize)]
struct FindArgs {
    pattern: String,
    path: Option<PathBuf>,
}

#[derive(Deserialize)]
struct GrepArgs {
    pattern: String,
    path: Option<PathBuf>,
    glob: Option<String>,
    #[serde(default)]
    ignore_case: bool,
}

#[derive(Deserialize)]
struct ShellArgs {
    command: String,
    timeout_seconds: Option<u64>,
    #[serde(default)]
    run_in_background: bool,
}

#[derive(Deserialize)]
struct BackgroundArgs {
    id: String,
}

#[derive(Deserialize)]
struct FetchArgs {
    url: String,
    prompt: Option<String>,
}

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
}

fn arguments<T: DeserializeOwned>(name: &str, arguments: &Value) -> Result<T, String> {
    T::deserialize(arguments).map_err(|error| format!("{BAD_ARGUMENTS_TEXT} {name}: {error}"))
}

fn refused(name: &str, reason: impl std::fmt::Display) -> String {
    format!("{BAD_ARGUMENTS_TEXT} {name}: {reason}")
}

impl Action {
    pub(crate) fn parse(name: &str, value: &Value) -> Result<Self, String> {
        Ok(match name {
            READ_TOOL => {
                let ReadArgs {
                    path,
                    offset,
                    limit,
                } = arguments(name, value)?;
                Self::Read {
                    path,
                    offset,
                    limit,
                }
            }
            WRITE_TOOL => {
                let WriteArgs { path, content } = arguments(name, value)?;
                Self::Write { path, content }
            }
            EDIT_TOOL => {
                let args: EditArgs = arguments(name, value)?;
                args.into_action()
                    .ok_or_else(|| refused(name, NO_EDITS_TEXT))?
            }
            FIND_TOOL => {
                let FindArgs { pattern, path } = arguments(name, value)?;
                Self::Find { pattern, path }
            }
            GREP_TOOL => {
                let args: GrepArgs = arguments(name, value)?;
                Self::Grep {
                    pattern: args.pattern,
                    path: args.path,
                    glob: args.glob,
                    ignore_case: args.ignore_case,
                }
            }
            SHELL_TOOL => {
                let args: ShellArgs = arguments(name, value)?;
                Self::Shell {
                    command: args.command,
                    timeout: args.timeout_seconds.map(Duration::from_secs),
                    run: if args.run_in_background {
                        ShellRun::Background
                    } else {
                        ShellRun::Wait
                    },
                }
            }
            SHELL_OUTPUT_TOOL => {
                let BackgroundArgs { id } = arguments(name, value)?;
                Self::ShellOutput { id }
            }
            SHELL_STOP_TOOL => {
                let BackgroundArgs { id } = arguments(name, value)?;
                Self::ShellStop { id }
            }
            WEB_FETCH_TOOL => {
                let FetchArgs { url, prompt } = arguments(name, value)?;
                let url: WebUrl = url.parse().map_err(|error| refused(name, error))?;
                Self::WebFetch {
                    url,
                    prompt: prompt.filter(|prompt| !prompt.trim().is_empty()),
                }
            }
            WEB_SEARCH_TOOL => {
                let SearchArgs { query } = arguments(name, value)?;
                if query.trim().is_empty() {
                    return Err(refused(name, EMPTY_QUERY_TEXT));
                }
                Self::WebSearch { query }
            }
            mcp if is_mcp_tool(mcp) => Self::Mcp {
                name: mcp.to_owned(),
                arguments: value.clone(),
            },
            other => return Err(format!("{UNKNOWN_TOOL_TEXT} {other}")),
        })
    }
}
