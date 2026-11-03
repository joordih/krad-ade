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

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct InputField {
    pub id: String,
    pub label: String,
    pub value: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct InputParams {
    pub(crate) title: String,
    pub(crate) fields: Vec<InputField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerInput {
    pub id: ChoiceId,
    pub title: String,
    pub fields: Vec<InputField>,
}

impl ServerInput {
    pub(crate) fn new(id: Value, params: InputParams) -> Self {
        Self {
            id: ChoiceId(id),
            title: params.title,
            fields: params.fields,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Conflict {
    pub message: String,
    #[serde(default)]
    pub location: Option<lsp_types::Location>,
}

#[derive(serde::Deserialize)]
pub(crate) struct ConflictsParams {
    pub(crate) title: String,
    pub(crate) conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerConflicts {
    pub id: ChoiceId,
    pub title: String,
    pub conflicts: Vec<Conflict>,
}

impl ServerConflicts {
    pub(crate) fn new(id: Value, params: ConflictsParams) -> Self {
        Self {
            id: ChoiceId(id),
            title: params.title,
            conflicts: params.conflicts,
        }
    }
}
