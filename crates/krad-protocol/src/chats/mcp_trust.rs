use krad_domain::McpProjectFile;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct McpOffered {
    pub name: String,
    pub summary: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct McpSkippedShown {
    pub name: String,
    pub reason: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct McpTrustAsk {
    pub folder: PathBuf,
    pub servers: Vec<McpOffered>,
    pub skipped: Vec<McpSkippedShown>,
}

impl McpTrustAsk {
    #[must_use]
    pub fn new(folder: PathBuf, file: &McpProjectFile) -> Self {
        Self {
            folder,
            servers: file
                .servers
                .iter()
                .map(|server| McpOffered {
                    name: server.name.to_string(),
                    summary: server.summary(),
                })
                .collect(),
            skipped: file
                .skipped
                .iter()
                .map(|skipped| McpSkippedShown {
                    name: skipped.name.clone(),
                    reason: skipped.reason.to_string(),
                })
                .collect(),
        }
    }
}
