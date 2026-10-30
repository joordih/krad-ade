use serde_json::json;
use std::borrow::Cow;

use super::{
    EDIT_TOOL, FIND_TOOL, GREP_TOOL, READ_TOOL, SHELL_OUTPUT_TOOL, SHELL_STOP_TOOL, SHELL_TOOL,
    WEB_FETCH_TOOL, WEB_SEARCH_TOOL, WRITE_TOOL,
};
use crate::engine::wire::ToolSpec;

const READ_DESCRIPTION: &str = "Read a text file. Lines come numbered. Use offset (1-based line) and limit to page through large files.";
const WRITE_DESCRIPTION: &str = "Write a whole file, creating it and its folders when missing and replacing it otherwise. Prefer edit for changes to existing files.";
const EDIT_DESCRIPTION: &str = "Replace exact text in a file. old_string must match the file exactly, including whitespace, and appear once unless replace_all is true. To make several changes to one file, pass edits, a list of {old_string, new_string, replace_all} applied in order; either all of them apply or none does.";
const FIND_DESCRIPTION: &str = "Find files by glob pattern, such as **/*.rs, below a folder (the working folder by default). Ignored files are skipped.";
const GREP_DESCRIPTION: &str = "Search file contents with a regular expression below a folder or in one file. Returns path:line:text. Ignored files are skipped.";
const SHELL_DESCRIPTION: &str = "Run a shell command in the working folder and return its output and exit code. Use it for builds, tests, git and other programs. Set run_in_background for long-running processes such as dev servers or watchers: it returns an id at once, then read the output with shell_output and stop the process with shell_stop.";
const SHELL_OUTPUT_DESCRIPTION: &str = "Read the new output of a background shell since the last read, and whether it is still running or its exit code.";
const SHELL_STOP_DESCRIPTION: &str =
    "Stop a background shell and everything it started, returning its last output.";
const WEB_FETCH_DESCRIPTION: &str = "Fetch a web page over http or https and return it as readable text, with links as text (url). Use it to read documentation, issues or pages the user mentions. Redirects to another host are reported, not followed.";
const WEB_SEARCH_DESCRIPTION: &str = "Search the web and return an answer with its source links. Use it for current information, such as library versions, release notes or error messages, then fetch a source with web_fetch to read it in full.";

pub(crate) fn specs() -> Vec<ToolSpec> {
    file_specs()
        .into_iter()
        .chain(shell_specs())
        .chain(web_specs())
        .collect()
}

fn file_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: Cow::Borrowed(READ_TOOL),
            description: Cow::Borrowed(READ_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path, relative to the working folder or absolute"},
                    "offset": {"type": "integer", "description": "First line to read, from 1"},
                    "limit": {"type": "integer", "description": "Most lines to read"}
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: Cow::Borrowed(WRITE_TOOL),
            description: Cow::Borrowed(WRITE_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path, relative to the working folder or absolute"},
                    "content": {"type": "string", "description": "The whole new content of the file"}
                },
                "required": ["path", "content"]
            }),
        },
        ToolSpec {
            name: Cow::Borrowed(EDIT_TOOL),
            description: Cow::Borrowed(EDIT_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path, relative to the working folder or absolute"},
                    "old_string": {"type": "string", "description": "Exact text to replace"},
                    "new_string": {"type": "string", "description": "Text to put in its place"},
                    "replace_all": {"type": "boolean", "description": "Replace every occurrence instead of exactly one"},
                    "edits": {
                        "type": "array",
                        "description": "Several replacements applied in order, instead of old_string and new_string",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_string": {"type": "string", "description": "Exact text to replace"},
                                "new_string": {"type": "string", "description": "Text to put in its place"},
                                "replace_all": {"type": "boolean", "description": "Replace every occurrence instead of exactly one"}
                            },
                            "required": ["old_string", "new_string"]
                        }
                    }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: Cow::Borrowed(FIND_TOOL),
            description: Cow::Borrowed(FIND_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Glob pattern matched against paths below the folder"},
                    "path": {"type": "string", "description": "Folder to search, the working folder by default"}
                },
                "required": ["pattern"]
            }),
        },
        ToolSpec {
            name: Cow::Borrowed(GREP_TOOL),
            description: Cow::Borrowed(GREP_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regular expression"},
                    "path": {"type": "string", "description": "Folder or file to search, the working folder by default"},
                    "glob": {"type": "string", "description": "Only search files matching this glob, such as *.rs"},
                    "ignore_case": {"type": "boolean", "description": "Match without regard to case"}
                },
                "required": ["pattern"]
            }),
        },
    ]
}

fn shell_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: Cow::Borrowed(SHELL_TOOL),
            description: Cow::Borrowed(SHELL_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "The command line to run"},
                    "timeout_seconds": {"type": "integer", "description": "Seconds before the command is stopped, 120 by default, at most 600"},
                    "run_in_background": {"type": "boolean", "description": "Start the command and return its id at once instead of waiting for it"}
                },
                "required": ["command"]
            }),
        },
        ToolSpec {
            name: Cow::Borrowed(SHELL_OUTPUT_TOOL),
            description: Cow::Borrowed(SHELL_OUTPUT_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "The id the background shell was started with, such as bg1"}
                },
                "required": ["id"]
            }),
        },
        ToolSpec {
            name: Cow::Borrowed(SHELL_STOP_TOOL),
            description: Cow::Borrowed(SHELL_STOP_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "The id the background shell was started with, such as bg1"}
                },
                "required": ["id"]
            }),
        },
    ]
}

fn web_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: Cow::Borrowed(WEB_FETCH_TOOL),
            description: Cow::Borrowed(WEB_FETCH_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "The http or https address of the page"},
                    "prompt": {"type": "string", "description": "What to look for in the page"}
                },
                "required": ["url"]
            }),
        },
        ToolSpec {
            name: Cow::Borrowed(WEB_SEARCH_TOOL),
            description: Cow::Borrowed(WEB_SEARCH_DESCRIPTION),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "What to search for"}
                },
                "required": ["query"]
            }),
        },
    ]
}
