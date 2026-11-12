use krad_domain::{EnvironmentChange, EnvironmentData, EnvironmentName};
use krad_protocol::EnvironmentCommand;

#[derive(Default)]
pub(in crate::environment_shell) struct EnvironmentsMirror(Vec<EnvironmentData>);

fn names(environments: &[EnvironmentData]) -> Vec<&EnvironmentName> {
    environments
        .iter()
        .map(|environment| &environment.name)
        .collect()
}

impl EnvironmentsMirror {
    pub(in crate::environment_shell) fn replace(&mut self, environments: Vec<EnvironmentData>) {
        self.0 = environments;
    }

    pub(super) fn opened(&mut self, environment: EnvironmentData) {
        match self.find_mut(&environment.name) {
            Some(kept) => *kept = environment,
            None => self.0.push(environment),
        }
    }

    pub(super) fn closed(&mut self, name: &EnvironmentName) {
        self.0.retain(|kept| kept.name != *name);
    }

    pub(super) fn ordered(&mut self, order: &[EnvironmentName]) {
        self.0.sort_by_key(|kept| {
            order
                .iter()
                .position(|name| *name == kept.name)
                .unwrap_or(order.len())
        });
    }

    pub(super) fn changed(&mut self, name: &EnvironmentName, changes: Vec<EnvironmentChange>) {
        if let Some(kept) = self.find_mut(name) {
            for change in changes {
                kept.apply(change);
            }
        }
    }

    fn find_mut(&mut self, name: &EnvironmentName) -> Option<&mut EnvironmentData> {
        self.0.iter_mut().find(|kept| kept.name == *name)
    }

    fn command_toward(&self, environment: &EnvironmentData) -> Option<EnvironmentCommand> {
        let Some(kept) = self.0.iter().find(|kept| kept.name == environment.name) else {
            let mut opened = environment.clone();
            opened.take_view();
            return Some(EnvironmentCommand::Open {
                environment: Box::new(opened),
            });
        };
        let changes: Vec<EnvironmentChange> = EnvironmentChange::between(kept, environment)
            .into_iter()
            .filter(|change| !change.is_runtime_owned())
            .collect();
        (!changes.is_empty()).then(|| EnvironmentCommand::Change {
            environment: environment.name.clone(),
            changes,
        })
    }

    pub(in crate::environment_shell) fn commands_toward(
        &self,
        wanted: &[EnvironmentData],
    ) -> Vec<EnvironmentCommand> {
        let mut commands: Vec<EnvironmentCommand> = self
            .0
            .iter()
            .filter(|kept| {
                !wanted
                    .iter()
                    .any(|environment| environment.name == kept.name)
            })
            .map(|kept| EnvironmentCommand::Close {
                environment: kept.name.clone(),
            })
            .collect();
        commands.extend(
            wanted
                .iter()
                .filter_map(|environment| self.command_toward(environment)),
        );
        let mut after: Vec<&EnvironmentName> = names(&self.0)
            .into_iter()
            .filter(|name| wanted.iter().any(|environment| environment.name == **name))
            .collect();
        after.extend(
            names(wanted)
                .into_iter()
                .filter(|name| !self.0.iter().any(|kept| kept.name == **name)),
        );
        let order = names(wanted);
        if after != order {
            commands.push(EnvironmentCommand::Order {
                order: order.into_iter().cloned().collect(),
            });
        }
        commands
    }
}

#[cfg(test)]
mod tests;
