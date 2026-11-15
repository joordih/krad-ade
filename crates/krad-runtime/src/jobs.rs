use krad_domain::{
    AgentPreset, AgentState, BufferVersion, DirEntry, EnvironmentName, HarnessHealth, Host,
    HostPlatform, ModelOffer, PlanUsage, UsageFailure,
};
use krad_host::HostError;
use krad_protocol::{AgentRef, Answer, BufferId, HealthPlace, RequestId};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::thread;

use crate::adapters::AdapterDone;
use crate::chats::{
    AcpSecretsRead, ChatCheckpoint, ChatEarlier, ChatOpening, FilesIndexed, McpLaunched, McpRead,
};
use crate::cloud::CloudDone;
use crate::cloud_runs::LivenessChecked;
use crate::code_hosts::Observed;
use crate::gateway::{GatewayForgotten, GatewayRead};
use crate::git::GitJob;
use crate::hosts::{AgentsFound, Attempt, LocalFound, RemoteBus};
use crate::input::Input;
use crate::models::AcpOffersRead;
use crate::opencode::{CloudOpencodeStarted, OpencodeListed, OpencodeStarted};
use crate::reviews::ReviewDone;
use crate::runners::RunnerDone;
use crate::runtime::{AttachedLine, ChatImageDone, LinesCounted, SignInEnvRead};
use crate::usage::UsageKey;
use crate::variants::VariantJob;

const JOB_THREAD: &str = "krad-runtime-job";

#[derive(Debug)]
pub(crate) enum BufferJob {
    Read {
        place: (EnvironmentName, PathBuf),
        read: Result<String, String>,
    },
    Written {
        buffer: BufferId,
        version: BufferVersion,
        environment: EnvironmentName,
        written: Result<(), String>,
    },
}

#[derive(Debug)]
pub(crate) enum JobDone {
    Buffer(BufferJob),
    FolderListed {
        folder: (EnvironmentName, PathBuf),
        listed: Result<Vec<DirEntry>, String>,
    },
    Answered {
        request: RequestId,
        answer: Box<Answer>,
    },
    Uploaded {
        request: RequestId,
        answer: Box<Answer>,
    },
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
    ChatImage(ChatImageDone),
    FilesIndexed(FilesIndexed),
    GatewayChecked(GatewayRead),
    GatewayForgotten(GatewayForgotten),
    OpencodeStarted(OpencodeStarted),
    OpencodeListed(OpencodeListed),
    AcpSecretsRead(AcpSecretsRead),
    AcpOffersRead(AcpOffersRead),
    AttachedLine(AttachedLine),
    OpencodeTerminal {
        terminal: AgentRef,
        state: AgentState,
    },
    SignInEnvRead(SignInEnvRead),
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
    CloudLiveness(LivenessChecked),
    CloudOpencodeStarted(CloudOpencodeStarted),
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

    pub(crate) fn steps(&self) -> Steps {
        Steps(self.0.clone())
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
