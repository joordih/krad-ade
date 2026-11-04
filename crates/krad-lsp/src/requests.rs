use lsp_types::request::Request;
use lsp_types::Uri;
use serde::{Deserialize, Serialize};

pub enum DocumentContentRequest {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentContentParams {
    pub uri: Uri,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentContent {
    pub text: String,
    pub language_id: String,
}

impl Request for DocumentContentRequest {
    type Params = DocumentContentParams;
    type Result = Option<DocumentContent>;
    const METHOD: &'static str = "krad/documentContent";
}

pub const FILE_TEMPLATES_CAPABILITY: &str = "kradFileTemplates";

#[must_use]
pub fn offers_file_templates(capabilities: &lsp_types::ServerCapabilities) -> bool {
    capabilities
        .experimental
        .as_ref()
        .and_then(|experimental| experimental.get(FILE_TEMPLATES_CAPABILITY))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub enum FileTemplatesRequest {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTemplatesParams {
    pub folder: Uri,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTemplate {
    pub id: String,
    pub label: String,
    pub group: String,
}

impl Request for FileTemplatesRequest {
    type Params = FileTemplatesParams;
    type Result = Vec<FileTemplate>;
    const METHOD: &'static str = "krad/fileTemplates";
}

pub enum CreateFromTemplateRequest {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateFromTemplateParams {
    pub folder: Uri,
    pub template: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateCreation {
    pub edit: lsp_types::WorkspaceEdit,
    pub folders: Vec<Uri>,
    pub open: Option<lsp_types::Location>,
}

impl Request for CreateFromTemplateRequest {
    type Params = CreateFromTemplateParams;
    type Result = Option<TemplateCreation>;
    const METHOD: &'static str = "krad/createFromTemplate";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_server_that_says_so_offers_file_templates() -> serde_json::Result<()> {
        let offering: lsp_types::ServerCapabilities =
            serde_json::from_str(r#"{"experimental": {"kradFileTemplates": true}}"#)?;
        let silent: lsp_types::ServerCapabilities = serde_json::from_str("{}")?;
        assert!(offers_file_templates(&offering));
        assert!(!offers_file_templates(&silent));
        Ok(())
    }
}
