use krad_domain::{EnvironmentChange, EnvironmentName, VariantRuns};
use krad_protocol::EventKind;

use super::Environments;

impl Environments {
    pub(crate) fn variant_runs(
        &mut self,
        environment: &EnvironmentName,
        act: impl FnOnce(&mut VariantRuns) -> bool,
        outbox: &mut Vec<EventKind>,
    ) -> Option<bool> {
        let kept = self.find_mut(environment)?;
        if !act(&mut kept.variant_runs) {
            return Some(false);
        }
        let runs = kept.variant_runs.clone();
        self.dirty = true;
        outbox.push(EventKind::EnvironmentChanged {
            environment: environment.clone(),
            changes: vec![EnvironmentChange::VariantRuns { runs }],
            answers: None,
        });
        Some(true)
    }
}
