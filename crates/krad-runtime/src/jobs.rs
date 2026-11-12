use krad_domain::{AgentPreset, HarnessHealth, Host, ModelOffer, PlanUsage, UsageFailure};
use krad_protocol::HealthPlace;
use std::io;
use std::sync::mpsc::SyncSender;
use std::thread;

use crate::input::Input;
use crate::usage::UsageKey;

const JOB_THREAD: &str = "krad-runtime-job";

#[derive(Debug)]
pub(crate) enum JobDone {
    HarnessChecked {
        place: HealthPlace,
        harness: AgentPreset,
        health: HarnessHealth,
    },
    ModelsListed {
        key: (Host, AgentPreset),
        listed: Result<Vec<ModelOffer>, String>,
    },
    UsageRead {
        key: UsageKey,
        read: Result<PlanUsage, UsageFailure>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Jobs(SyncSender<Input>);

impl Jobs {
    pub(crate) const fn new(inbox: SyncSender<Input>) -> Self {
        Self(inbox)
    }

    pub(crate) fn run(&self, work: impl FnOnce() -> JobDone + Send + 'static) -> io::Result<()> {
        let inbox = self.0.clone();
        thread::Builder::new()
            .name(JOB_THREAD.to_owned())
            .spawn(move || {
                if inbox.send(Input::JobDone(work())).is_err() {
                    tracing::trace!("a job finished after the runtime stopped");
                }
            })
            .map(drop)
    }
}
