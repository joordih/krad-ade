use futures::channel::mpsc::UnboundedSender;
use gpui_kit::{div, AnyView, Context, IntoElement, Render, Window};
use krad_extensions::api::protocol::RowsResult;
use krad_extensions::api::view::{apply, Applied, Node, Patch, ViewEvent};
use krad_extensions::api::{NodeKey, TerminalHandle};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ViewOutput {
    Event(ViewEvent),
    Rows { node: NodeKey, start: u32, end: u32 },
}

pub(crate) struct ExtensionView {
    root: Node,
    rows: HashMap<NodeKey, RowsResult>,
    terminals: HashMap<TerminalHandle, AnyView>,
    output: UnboundedSender<ViewOutput>,
}

impl ExtensionView {
    pub(crate) fn new(root: Node, output: UnboundedSender<ViewOutput>) -> Self {
        Self {
            root,
            rows: HashMap::new(),
            terminals: HashMap::new(),
            output,
        }
    }

    pub(crate) fn replace_root(&mut self, root: Node, cx: &mut Context<Self>) {
        self.root = root;
        self.rows.clear();
        cx.notify();
    }

    pub(crate) fn apply(&mut self, patches: Vec<Patch>, cx: &mut Context<Self>) {
        for patch in patches {
            match apply(&mut self.root, patch) {
                Ok(Applied::Changed) => {}
                Ok(Applied::RowsInvalidated(node)) => {
                    self.rows.remove(&node);
                }
                Err(error) => tracing::debug!(%error, "extension view patch refused"),
            }
        }
        cx.notify();
    }

    pub(crate) fn set_rows(&mut self, node: NodeKey, rows: RowsResult, cx: &mut Context<Self>) {
        self.rows.insert(node, rows);
        cx.notify();
    }

    pub(crate) fn set_terminal(
        &mut self,
        handle: TerminalHandle,
        view: AnyView,
        cx: &mut Context<Self>,
    ) {
        self.terminals.insert(handle, view);
        cx.notify();
    }

    fn send(&self, output: ViewOutput) {
        if self.output.unbounded_send(output).is_err() {
            tracing::trace!("extension view output dropped after its program stopped");
        }
    }
}

impl Render for ExtensionView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if self.root.children().is_empty() {
            self.send(ViewOutput::Rows {
                node: self.root.key.clone(),
                start: 0,
                end: 0,
            });
        }
        div()
    }
}
