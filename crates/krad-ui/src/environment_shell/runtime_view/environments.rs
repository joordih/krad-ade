use krad_domain::{EnvironmentChange, EnvironmentData, EnvironmentName};

#[derive(Clone, Default)]
pub(in crate::environment_shell) struct EnvironmentsMirror(Vec<EnvironmentData>);

impl EnvironmentsMirror {
    pub(in crate::environment_shell) fn replace(&mut self, environments: Vec<EnvironmentData>) {
        self.0 = environments;
    }

    pub(in crate::environment_shell) fn all(&self) -> &[EnvironmentData] {
        &self.0
    }

    pub(in crate::environment_shell) fn get(
        &self,
        name: &EnvironmentName,
    ) -> Option<&EnvironmentData> {
        self.0.iter().find(|kept| kept.name == *name)
    }

    pub(in crate::environment_shell) fn names(&self) -> Vec<EnvironmentName> {
        self.0.iter().map(|kept| kept.name.clone()).collect()
    }

    pub(in crate::environment_shell) fn opened(&mut self, environment: EnvironmentData) {
        match self.find_mut(&environment.name) {
            Some(kept) => *kept = environment,
            None => self.0.push(environment),
        }
    }

    pub(in crate::environment_shell) fn closed(&mut self, name: &EnvironmentName) {
        self.0.retain(|kept| kept.name != *name);
    }

    pub(in crate::environment_shell) fn ordered(&mut self, order: &[EnvironmentName]) {
        self.0.sort_by_key(|kept| {
            order
                .iter()
                .position(|name| *name == kept.name)
                .unwrap_or(order.len())
        });
    }

    pub(in crate::environment_shell) fn changed(
        &mut self,
        name: &EnvironmentName,
        changes: Vec<EnvironmentChange>,
    ) {
        if let Some(kept) = self.find_mut(name) {
            for change in changes {
                kept.apply(change);
            }
        }
    }

    fn find_mut(&mut self, name: &EnvironmentName) -> Option<&mut EnvironmentData> {
        self.0.iter_mut().find(|kept| kept.name == *name)
    }
}
