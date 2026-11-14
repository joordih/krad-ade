use krad_domain::{EnvironmentChange, EnvironmentData, EnvironmentName};
use krad_protocol::RequestId;
use std::rc::Rc;

use super::environment_copy::{EnvironmentCopy, NodesMoved};
use super::EnvironmentsMirror;

#[derive(Clone, Default)]
pub(in crate::environment_shell) struct ShownEnvironments(Vec<Rc<EnvironmentCopy>>);

impl ShownEnvironments {
    pub(in crate::environment_shell) fn replace(&mut self, environments: Vec<EnvironmentData>) {
        self.0 = environments
            .into_iter()
            .map(|data| Rc::new(EnvironmentCopy::new(data)))
            .collect();
    }

    pub(in crate::environment_shell) fn names(&self) -> Vec<EnvironmentName> {
        self.0.iter().map(|copy| copy.name.clone()).collect()
    }

    pub(in crate::environment_shell) fn get(
        &self,
        name: &EnvironmentName,
    ) -> Option<&Rc<EnvironmentCopy>> {
        self.0.iter().find(|copy| copy.name == *name)
    }

    fn position(&self, name: &EnvironmentName) -> Option<usize> {
        self.0.iter().position(|copy| copy.name == *name)
    }

    fn copy_mut(&mut self, name: &EnvironmentName) -> Option<&mut EnvironmentCopy> {
        self.0
            .iter_mut()
            .find(|copy| copy.name == *name)
            .map(Rc::make_mut)
    }

    pub(in crate::environment_shell) fn open(&mut self, data: EnvironmentData) {
        if self.position(&data.name).is_none() {
            self.0.push(Rc::new(EnvironmentCopy::new(data)));
        }
    }

    pub(in crate::environment_shell) fn edit<R>(
        &mut self,
        name: &EnvironmentName,
        held: &mut Rc<EnvironmentCopy>,
        edit: impl FnOnce(&mut EnvironmentCopy) -> R,
    ) -> Option<R> {
        let at = self.position(name)?;
        *held = self.0.remove(at);
        let edited = edit(Rc::make_mut(held));
        self.0.insert(at, Rc::clone(held));
        Some(edited)
    }

    pub(in crate::environment_shell) fn opened(&mut self, runtime: &EnvironmentData) -> NodesMoved {
        if let Some(copy) = self.copy_mut(&runtime.name) {
            return copy.adopt(runtime);
        }
        self.0.push(Rc::new(EnvironmentCopy::new(runtime.clone())));
        NodesMoved::default()
    }

    pub(in crate::environment_shell) fn heard(
        &mut self,
        name: &EnvironmentName,
        changes: &[EnvironmentChange],
        answers: Option<RequestId>,
        runtime: &EnvironmentData,
    ) -> NodesMoved {
        self.copy_mut(name)
            .map_or_else(NodesMoved::default, |copy| {
                copy.heard(changes, answers, runtime)
            })
    }

    pub(in crate::environment_shell) fn closed(&mut self, name: &EnvironmentName) {
        self.0.retain(|copy| copy.name != *name);
    }

    pub(in crate::environment_shell) fn ordered(&mut self, order: &[EnvironmentName]) {
        self.0.sort_by_key(|copy| {
            order
                .iter()
                .position(|name| *name == copy.name)
                .unwrap_or(order.len())
        });
    }

    pub(in crate::environment_shell) fn adopt(
        &mut self,
        runtime: &EnvironmentsMirror,
    ) -> Vec<(EnvironmentName, NodesMoved)> {
        runtime
            .all()
            .iter()
            .map(|data| (data.name.clone(), self.opened(data)))
            .filter(|(_, moved)| !moved.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests;
