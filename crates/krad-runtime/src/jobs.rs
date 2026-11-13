use krad_domain::{
    AgentPreset, EnvironmentName, HarnessHealth, Host, HostPlatform, ModelOffer, PlanUsage,
    UsageFailure,
};
use krad_host::HostError;
use krad_protocol::HealthPlace;
use std::io;
use std::sync::mpsc::SyncSender;
use std::thread;

use crate::adapters::AdapterDone;
use crate::chats::{
    AcpSecretsRead, ChatCheckpoint, ChatEarlier, ChatOpening, FilesIndexed, McpLaunched, McpRead,
};
use crate::cloud::CloudDone;
use crate::code_hosts::Observed;
use crate::gateway::{GatewayForgotten, GatewayRead};
use crate::git::GitJob;
use crate::hosts::{AgentsFound, Attempt, LocalFound, RemoteBus};
use crate::input::Input;
use crate::opencode::{OpencodeListed, OpencodeStarted};
use crate::reviews::ReviewDone;
use crate::runners::RunnerDone;
use crate::runtime::{AttachedLine, LinesCounted};
use crate::usage::UsageKey;
use crate::variants::VariantJob;

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
    HostConnected {
        environment: EnvironmentName,
        host: Host,
        attempt: Attempt,
        outcome: Result<HostPlatform, HostError>,
    },
    RemoteBusPrepared {
        environment: EnvironmentName,
        host: Host,
        prepared: Result<RemoteBus, String>,
    },
    ChatOpened(ChatOpening),
    ChatEarlier(ChatEarlier),
    ChatCheckpoint(ChatCheckpoint),
    FilesIndexed(FilesIndexed),
    GatewayChecked(GatewayRead),
    GatewayForgotten(GatewayForgotten),
    OpencodeStarted(OpencodeStarted),
    OpencodeListed(OpencodeListed),
    AcpSecretsRead(AcpSecretsRead),
    AttachedLine(AttachedLine),
    HostAgentsFound(AgentsFound),
    LocalAgentsFound(LocalFound),
    McpRead(McpRead),
    McpLaunched(McpLaunched),
    Git(GitJob),
    KraddieLines(LinesCounted),
    Adapter(AdapterDone),
    Variant(VariantJob),
    CodeHosts(Vec<Observed>),
    Review(Box<ReviewDone>),
    Runner(Box<RunnerDone>),
    Cloud(Box<CloudDone>),
}

#[derive(Clone, Debug)]
pub(crate) struct Jobs(SyncSender<Input>);

impl Jobs {
    pub(crate) const fn new(inbox: SyncSender<Input>) -> Self {
        Self(inbox)
    }

    pub(crate) fn run(&self, work: impl FnOnce() -> JobDone + Send + 'static) -> io::Result<()> {
        self.run_reporting(move |steps| steps.report(work()))
    }

    pub(crate) fn run_reporting(
        &self,
        work: impl FnOnce(&Steps) + Send + 'static,
    ) -> io::Result<()> {
        let steps = Steps(self.0.clone());
        thread::Builder::new()
            .name(JOB_THREAD.to_owned())
            .spawn(move || work(&steps))
            .map(drop)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Steps(SyncSender<Input>);

impl Steps {
    pub(crate) fn report(&self, done: JobDone) {
        if self.0.send(Input::JobDone(done)).is_err() {
            tracing::trace!("a job reported after the runtime stopped");
        }
    }
}
