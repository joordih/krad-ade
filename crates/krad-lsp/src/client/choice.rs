use lsp_types::ShowMessageRequestParams;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceId(pub(crate) Value);

#[derive(Debug, Clone, PartialEq)]
pub struct ServerChoice {
    pub id: ChoiceId,
    pub message: String,
    pub options: Vec<String>,
}

impl ServerChoice {
    pub(crate) fn new(id: Value, params: ShowMessageRequestParams) -> Self {
        Self {
            id: ChoiceId(id),
            message: params.message,
            options: params
                .actions
                .unwrap_or_default()
                .into_iter()
                .map(|action| action.title)
                .collect(),
        }
    }
}
