use std::time::Instant;

use super::Runtime;

impl Runtime {
    pub(super) fn tick(&mut self) {
        if let Some(bus) = &mut self.bus {
            bus.tick(&mut self.outbox);
        }
        self.usage.tick(&self.jobs, &mut self.outbox);
        self.terminals.tick(Instant::now(), &mut self.outbox);
        self.type_due(Instant::now());
        self.watch_runner_buses();
        self.sync_runner_webhooks(Instant::now());
        self.hear_runner_sign_ins();
        self.keep_cloud_offsets(Instant::now());
        self.tick_hosts();
        self.tick_git();
        self.tick_kraddies();
        self.tick_code_hosts();
    }
}
