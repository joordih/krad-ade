use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::id::{NodeKey, ViewId, ViewInstance};
use crate::protocol::{notification, request, Empty};
use crate::view::{ListRow, Node, Patch, TableRow, ViewEvent};

pub const MAX_ROWS_PER_REQUEST: u32 = 500;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ViewRef {
    pub view: ViewId,
    #[serde(default)]
    pub instance: Option<ViewInstance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenViewParams {
    #[serde(flatten)]
    pub view: ViewRef,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PatchViewParams {
    #[serde(flatten)]
    pub view: ViewRef,
    pub patches: Vec<Patch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewEventParams {
    #[serde(flatten)]
    pub view: ViewRef,
    pub event: ViewEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RowsParams {
    #[serde(flatten)]
    pub view: ViewRef,
    pub node: NodeKey,
    pub start: u32,
    pub end: u32,
}

impl RowsParams {
    #[must_use]
    pub fn bounded_end(&self) -> u32 {
        self.end
            .min(self.start.saturating_add(MAX_ROWS_PER_REQUEST))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowsResult {
    List { rows: Vec<ListRow> },
    Table { rows: Vec<TableRow> },
}

request!(RenderView, "view/render", Program, ViewRef, Node);
request!(Rows, "view/rows", Program, RowsParams, RowsResult);
notification!(
    ViewEventNotification,
    "view/event",
    Program,
    ViewEventParams
);
notification!(ViewClosed, "view/closed", Program, ViewRef);
request!(OpenView, "view/open", App, OpenViewParams, Empty);
request!(CloseView, "view/close", App, ViewRef, Empty);
notification!(PatchView, "view/patch", App, PatchViewParams);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rows_request_never_asks_for_more_than_the_cap() -> Result<(), crate::IdError> {
        let params = RowsParams {
            view: ViewRef {
                view: "containers".parse()?,
                instance: None,
            },
            node: "list".parse()?,
            start: 10,
            end: 100_000,
        };
        assert_eq!(params.bounded_end(), 10 + MAX_ROWS_PER_REQUEST);
        Ok(())
    }
}
