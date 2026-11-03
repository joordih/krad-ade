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
