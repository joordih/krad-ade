mod asks;
mod choices;
mod opening;
mod process;
mod prompt;
mod servers;
mod translate;
mod worker;

use krad_domain::{AcpAgentStart, AcpModeId, AcpModelId, ChatEvent, McpServerStart};
use krad_host::HostLink;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use thiserror::Error;
use tracing::debug;

use crate::acp::client_side::{ClientSide, FileWritten, OpenBuffers};
use crate::acp::config::SetSessionConfigOptionResponse;
use crate::acp::connection::{AcpClient, AcpConnection, AcpError, AgentRequest, Pending};
use crate::acp::paths::AgentPaths;
use crate::acp::prompt::PromptResponse;
use crate::acp::rpc::EmptyResponse;
use crate::acp::update::SessionNotification;
use crate::control::LiveEvent;
use crate::live::{BusVars, LiveHarness, LiveSession, Order};
use crate::process::Runner;
use process::AgentProcess;
use servers::{agent_env, MissingSecret};
use worker::Worker;

#[derive(Clone)]
pub struct AcpConfig {
    pub runner: Runner,
    pub agent: AcpAgentStart,
    pub cwd: PathBuf,
    pub mcp: Vec<McpServerStart>,
    pub bus_vars: BusVars,
    pub buffers: Arc<dyn OpenBuffers>,
}

#[derive(Debug, Error)]
enum Unstarted {
    #[error(transparent)]
    Secret(#[from] MissingSecret),
    #[error("cannot run {agent}: {error}")]
    Io { agent: String, error: io::Error },
}

#[derive(Debug)]
enum Answered {
    Prompted(Result<PromptResponse, AcpError>),
    ModeSet(AcpModeId, Result<EmptyResponse, AcpError>),
    ModelSet(AcpModelId, Result<EmptyResponse, AcpError>),
    Configured(Result<SetSessionConfigOptionResponse, AcpError>),
}

#[derive(Debug)]
enum Inbound {
    Order(Order),
    Update(SessionNotification),
    Asked(AgentRequest),
    Written(FileWritten),
    Answered(Answered),
    Loaded,
    Closed,
    Stop,
}

struct Forward(Sender<Inbound>);

impl Forward {
    fn send(&self, inbound: Inbound) {
        if self.0.send(inbound).is_err() {
            debug!("the ACP chat ended before a message from its agent");
        }
    }
}

impl AcpClient for Forward {
    fn session_update(&mut self, notification: SessionNotification) {
        self.send(Inbound::Update(notification));
    }

    fn request(&mut self, request: AgentRequest) {
        self.send(Inbound::Asked(request));
    }

    fn closed(&mut self) {
        self.send(Inbound::Closed);
    }
}

fn settle_later<R, F>(pending: Pending<R>, inbound: &Sender<Inbound>, answered: F)
where
    R: serde::de::DeserializeOwned + 'static,
    F: FnOnce(Result<R, AcpError>) -> Answered + Send + 'static,
{
    let inbound = inbound.clone();
    thread::spawn(move || {
        if inbound
            .send(Inbound::Answered(answered(pending.wait())))
            .is_err()
        {
            debug!("the ACP chat ended before its agent answered");
        }
    });
}

#[derive(Clone, Debug)]
struct Launch {
    runner: Runner,
    paths: AgentPaths,
    agent: String,
    mcp: Vec<McpServerStart>,
}

struct Serving {
    host: HostLink,
    buffers: Arc<dyn OpenBuffers>,
}

impl Serving {
    fn client_side(self, paths: AgentPaths, inbound: &Sender<Inbound>) -> ClientSide {
        let (written, files) = channel();
        let inbound = inbound.clone();
        thread::spawn(move || forward_written(&files, &inbound));
        ClientSide::new(self.host, paths, self.buffers, written)
    }
}

fn forward_written(files: &Receiver<FileWritten>, inbound: &Sender<Inbound>) {
    for written in files {
        if inbound.send(Inbound::Written(written)).is_err() {
            debug!("the ACP chat ended before a file its agent wrote");
            return;
        }
    }
}

pub(crate) struct AcpChat {
    orders: Sender<Inbound>,
    running: Arc<AtomicBool>,
    process: Arc<AgentProcess>,
}

impl LiveHarness for AcpChat {
    fn order(&self, order: Order) {
        if self.orders.send(Inbound::Order(order)).is_err() {
            debug!("an order came after the ACP chat ended");
        }
    }

    fn is_alive(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn stop(&self) {
        self.process.stop();
        if self.orders.send(Inbound::Stop).is_err() {
            debug!("the ACP chat had already ended");
        }
    }
}

fn connect(
    config: &AcpConfig,
    process: &AgentProcess,
    forward: Forward,
) -> Result<AcpConnection, Unstarted> {
    let mut env = agent_env(&config.agent)?;
    env.extend(config.bus_vars.0.iter().cloned());
    let pipes = process
        .launch(&config.runner, &config.cwd, &config.agent.agent.args, &env)
        .map_err(|error| Unstarted::Io {
            agent: config.agent.agent.display_name(),
            error,
        })?;
    Ok(AcpConnection::start(pipes.stdout, pipes.stdin, forward))
}

pub(crate) fn spawn_acp(
    config: AcpConfig,
    session: &LiveSession,
    sink: impl FnMut(LiveEvent) + Send + 'static,
) -> AcpChat {
    let launch = Launch {
        paths: AgentPaths::of(&config.runner, &config.cwd),
        runner: config.runner.clone(),
        agent: config.agent.agent.display_name(),
        mcp: config.mcp.clone(),
    };
    let serving = Serving {
        host: config.runner.link(),
        buffers: Arc::clone(&config.buffers),
    };
    let process = Arc::new(AgentProcess::default());
    let launching = Arc::clone(&process);
    spawn_worker(launch, serving, session, sink, process, move |forward| {
        connect(&config, &launching, forward)
    })
}

fn spawn_worker<S, C>(
    launch: Launch,
    serving: Serving,
    session: &LiveSession,
    mut sink: S,
    process: Arc<AgentProcess>,
    connect: C,
) -> AcpChat
where
    S: FnMut(LiveEvent) + Send + 'static,
    C: FnOnce(Forward) -> Result<AcpConnection, Unstarted> + Send + 'static,
{
    let (orders, inbox) = channel();
    let running = Arc::new(AtomicBool::new(true));
    let alive = Arc::clone(&running);
    let inbound = orders.clone();
    let session = session.clone();
    let working = Arc::clone(&process);
    thread::spawn(move || {
        match connect(Forward(inbound.clone())) {
            Ok(connection) => {
                let client_side = serving.client_side(launch.paths.clone(), &inbound);
                Worker::new(
                    launch,
                    connection,
                    client_side,
                    inbound,
                    working,
                    Arc::clone(&alive),
                    sink,
                )
                .run(&session, &inbox);
            }
            Err(error) => sink(LiveEvent::Chat(ChatEvent::Error(error.to_string()))),
        }
        alive.store(false, Ordering::SeqCst);
    });
    AcpChat {
        orders,
        running,
        process,
    }
}

#[cfg(test)]
mod tests;
