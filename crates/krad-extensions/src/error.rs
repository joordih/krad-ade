use krad_domain::ServerRuntimeId;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("download failed: {0}")]
    Http(#[from] ureq::Error),
    #[error("could not write to disk: {0}")]
    Io(#[from] io::Error),
    #[error("unexpected data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{file} is not a valid manifest: {reason}")]
    Manifest { file: String, reason: String },
    #[error("{url} does not match its published SHA-256")]
    Digest { url: String },
    #[error("`{0}` failed")]
    Tool(String),
    #[error("`{program}` failed{}: {}", exit_text(*.status), headline(output))]
    ToolFailed {
        program: String,
        status: Option<i32>,
        output: String,
    },
    #[error("{0} was not found after installing it")]
    Missing(String),
    #[error("no build for this platform")]
    NoBuild,
    #[error("language servers are not offered on this platform")]
    Unsupported,
    #[error("template `{0}` could not be filled")]
    Template(String),
    #[error("no server runtime installs `{0}` packages")]
    UnknownKind(String),
    #[error("the package registry has no `{0}`")]
    UnknownPackage(String),
    #[error("{0} is needed but was not found")]
    MissingServerRuntime(ServerRuntimeId),
    #[error(transparent)]
    Host(#[from] krad_host::HostError),
}

const ERROR_MARKERS: [&str; 5] = ["ERROR:", "error:", "Error:", "npm ERR!", "fatal:"];
const HEADLINE_MAX_CHARS: usize = 200;
const ELLIPSIS: char = '…';

fn headline(output: &str) -> String {
    let lines: Vec<&str> = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let chosen = lines
        .iter()
        .rev()
        .find_map(|line| {
            ERROR_MARKERS
                .iter()
                .find_map(|marker| line.strip_prefix(marker))
                .map(str::trim)
        })
        .or_else(|| lines.last().copied())
        .unwrap_or_default();
    if chosen.chars().count() <= HEADLINE_MAX_CHARS {
        return chosen.to_owned();
    }
    let mut cut: String = chosen.chars().take(HEADLINE_MAX_CHARS).collect();
    cut.push(ELLIPSIS);
    cut
}

impl ExtensionError {
    #[must_use]
    pub fn details(&self) -> Option<&str> {
        match self {
            Self::ToolFailed { output, .. } if !output.is_empty() => Some(output),
            _ => None,
        }
    }
}

fn exit_text(status: Option<i32>) -> String {
    status.map_or_else(String::new, |code| format!(" with exit code {code}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_step_shows_its_last_error_line_and_keeps_the_output() {
        let output = "Collecting basedpyright\nERROR: Could not find a version\nERROR: No matching distribution found for basedpyright==1.40.2\nWARNING: You are using pip version 21.2.4";
        let error = ExtensionError::ToolFailed {
            program: "python".to_owned(),
            status: Some(1),
            output: output.to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "`python` failed with exit code 1: No matching distribution found for basedpyright==1.40.2"
        );
        assert_eq!(error.details(), Some(output));
        assert_eq!(headline("only line"), "only line");
        assert_eq!(
            headline(&"x".repeat(500)).chars().count(),
            HEADLINE_MAX_CHARS + 1
        );
    }
}
