use krad_domain::{
    Edited, EnvironmentChange, EnvironmentData, EnvironmentEdit, EnvironmentView, ModelPrices,
    NodeKind, NoteTabs, ShownExtensionViews, SidePanelLayout, TerminalId, UsageReport, VariantRuns,
};
use krad_protocol::RequestId;
use std::ops::Deref;

pub(in crate::environment_shell) struct EnvironmentCopy {
    data: EnvironmentData,
    awaiting: Option<RequestId>,
    missed: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct NodesMoved {
    pub(in crate::environment_shell) gone: Vec<TerminalId>,
    pub(in crate::environment_shell) came: Vec<TerminalId>,
}

impl NodesMoved {
    pub(in crate::environment_shell) fn is_empty(&self) -> bool {
        self.gone.is_empty() && self.came.is_empty()
    }
}

fn nodes_of(data: &EnvironmentData) -> Vec<(TerminalId, NodeKind)> {
    data.nodes.iter().map(|node| (node.id, node.kind)).collect()
}

fn moved(before: &[(TerminalId, NodeKind)], after: &[(TerminalId, NodeKind)]) -> NodesMoved {
    NodesMoved {
        gone: before
            .iter()
            .filter(|node| !after.contains(node))
            .map(|(id, _)| *id)
            .collect(),
        came: after
            .iter()
            .filter(|node| !before.contains(node))
            .map(|(id, _)| *id)
            .collect(),
    }
}

impl Deref for EnvironmentCopy {
    type Target = EnvironmentData;

    fn deref(&self) -> &EnvironmentData {
        &self.data
    }
}

impl EnvironmentCopy {
    pub(in crate::environment_shell) const fn new(data: EnvironmentData) -> Self {
        Self {
            data,
            awaiting: None,
            missed: false,
        }
    }

    pub(in crate::environment_shell) fn edit(
        &mut self,
        edit: &EnvironmentEdit,
        request: RequestId,
    ) -> Edited {
        self.awaiting = Some(request);
        self.data.edit(edit)
    }

    pub(in crate::environment_shell) fn record_usage(
        &mut self,
        chat: TerminalId,
        report: &UsageReport,
        prices: &ModelPrices,
        request: RequestId,
    ) {
        self.awaiting = Some(request);
        self.data.record_usage(chat, report, prices);
    }

    pub(in crate::environment_shell) fn heard(
        &mut self,
        changes: &[EnvironmentChange],
        answers: Option<RequestId>,
        runtime: &EnvironmentData,
    ) -> NodesMoved {
        let answered = self
            .awaiting
            .zip(answers)
            .is_some_and(|(awaited, answers)| answers >= awaited);
        if answered {
            self.awaiting = None;
            if !std::mem::take(&mut self.missed) {
                return NodesMoved::default();
            }
            return self.adopt(runtime);
        }
        let owned = changes.iter().filter(|change| change.is_runtime_owned());
        if self.awaiting.is_none() {
            let before = nodes_of(&self.data);
            for change in owned {
                self.data.apply(change.clone());
            }
            return moved(&before, &nodes_of(&self.data));
        }
        if answers.is_none() {
            for change in owned {
                if change.is_terminal_meta() {
                    self.data.apply(change.clone());
                } else {
                    self.missed = true;
                }
            }
        }
        NodesMoved::default()
    }

    pub(in crate::environment_shell) fn adopt(&mut self, runtime: &EnvironmentData) -> NodesMoved {
        if self.awaiting.is_some() {
            self.missed = true;
            return NodesMoved::default();
        }
        let before = nodes_of(&self.data);
        self.data.adopt_runtime_items(runtime);
        moved(&before, &nodes_of(&self.data))
    }

    pub(in crate::environment_shell) fn set_view(&mut self, view: EnvironmentView) {
        self.data.view = view;
    }

    pub(in crate::environment_shell) fn set_title(&mut self, title: Option<String>) {
        self.data.title = title;
    }

    pub(in crate::environment_shell) const fn side_panel_mut(&mut self) -> &mut SidePanelLayout {
        &mut self.data.side_panel
    }

    pub(in crate::environment_shell) const fn note_tabs_mut(&mut self) -> &mut Vec<NoteTabs> {
        &mut self.data.note_tabs
    }

    pub(in crate::environment_shell) fn set_extension_views(&mut self, shown: ShownExtensionViews) {
        self.data.extension_views = shown;
    }

    pub(in crate::environment_shell) const fn variant_runs_mut(&mut self) -> &mut VariantRuns {
        &mut self.data.variant_runs
    }
}

#[cfg(test)]
mod tests;
