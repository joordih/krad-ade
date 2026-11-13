use super::Runtime;
use crate::jobs::JobDone;

impl Runtime {
    pub(super) fn job_done(&mut self, done: JobDone) {
        match done {
            JobDone::HarnessChecked {
                place,
                harness,
                health,
            } => self
                .health
                .checked(place, harness, health, &mut self.outbox),
            JobDone::ModelsListed { key, listed } => {
                self.models.listed(key, listed, &mut self.outbox);
            }
            JobDone::UsageRead { key, read } => self.usage.read(&key, read, &mut self.outbox),
            JobDone::Git(job) => self.git_job_done(job),
            JobDone::KraddieLines(counted) => self.kraddie_lines_counted(counted),
            JobDone::Variant(job) => self.variant_job_done(job),
            JobDone::CodeHosts(observed) => self.code_hosts_observed(observed),
            JobDone::Review(done) => self.review_done(*done),
            JobDone::Runner(done) => self.runner_done(*done),
            JobDone::Cloud(done) => self.cloud_done(*done),
            JobDone::Adapter(done) => {
                self.adapters.done(done, &self.jobs, &mut self.outbox);
                self.flush_chats();
            }
            host_job => self.host_job_done(host_job),
        }
    }
}
