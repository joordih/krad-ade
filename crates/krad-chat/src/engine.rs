mod agent;
mod attachments;
mod call;
mod checklist;
mod history;
mod mcp;
mod plan;
mod policy;
mod prompt;
mod provider;
mod questions;
mod session;
mod snapshots;
mod subagents;
mod tools;
mod wire;
mod worker;

use krad_domain::{
    ChatEntry, ChatEvent, ContextWindow, Effort, GatewayAccess, McpServerStart, ModelId,
    PermissionMode, ResumeId,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;

use crate::control::{Catalog, CommandOffer, LiveEvent};
use crate::gateway::GatewayModel;
use crate::live::{BusVars, LiveHarness, LiveSession, Order};
use crate::skills::skill_files;
use mcp::{McpStartFailure, McpTool, McpToolbox};
use prompt::{instruction_files, system_prompt, PromptParts};
use session::{read_records, refusal, session_path, Chosen, Record, Restored, SessionFile};
use snapshots::Snapshots;
use tools::{Background, Workspace};
use wire::ToolSpec;
use worker::{Inbound, Setup, Start, Worker};

pub use subagents::{engine_subagents, read_engine_subagent};
pub(crate) use wire::Protocol;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    pub access: GatewayAccess,
    pub sessions: PathBuf,
    pub home: Option<PathBuf>,
    pub cwd: PathBuf,
    pub model: Option<ModelId>,
    pub models: Vec<GatewayModel>,
    pub context_windows: HashMap<ModelId, ContextWindow>,
    pub system_prompt: Option<String>,
    pub effort: Option<Effort>,
    pub permission_mode: Option<PermissionMode>,
    pub bus_vars: BusVars,
    pub mcp: Vec<McpServerStart>,
}

const MCP_FAILED_LEAD: &str = "The MCP server ";
const MCP_FAILED_MIDDLE: &str = " did not start: ";
const MCP_DESCRIPTION_LEAD: &str = "Tool ";
const MCP_DESCRIPTION_MIDDLE: &str = " of the MCP server ";

pub(crate) struct EngineChat {
    orders: Sender<Inbound>,
    running: Arc<AtomicBool>,
    background: Background,
}

impl LiveHarness for EngineChat {
    fn order(&self, order: Order) {
        self.orders.send(Inbound::Order(order)).unwrap_or_default();
    }

    fn is_alive(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn stop(&self) {
        self.orders.send(Inbound::Stop).unwrap_or_default();
        self.background.stop_all();
    }
}

fn mcp_spec(tool: &McpTool) -> ToolSpec {
    let description = if tool.description.trim().is_empty() {
        format!(
            "{MCP_DESCRIPTION_LEAD}{}{MCP_DESCRIPTION_MIDDLE}{}",
            tool.tool, tool.server
        )
    } else {
        tool.description.clone()
    };
    ToolSpec {
        name: Cow::Owned(tool.name.clone()),
        description: Cow::Owned(description),
        parameters: tool.parameters.clone(),
    }
}

fn failure_notice(failure: &McpStartFailure) -> String {
    format!(
        "{MCP_FAILED_LEAD}{}{MCP_FAILED_MIDDLE}{}",
        failure.server, failure.error
    )
}

fn setup(config: &EngineConfig, mcp: Arc<McpToolbox>) -> Setup {
    let skills = skill_files(config.home.as_deref(), Some(&config.cwd));
    let instructions = instruction_files(config.home.as_deref(), &config.cwd);
    let system = system_prompt(&PromptParts {
        cwd: &config.cwd,
        instructions: &instructions,
        skills: &skills,
        extra: config.system_prompt.as_deref(),
    });
    Setup {
        access: config.access.clone(),
        workspace: Workspace {
            cwd: config.cwd.clone(),
            env: config.bus_vars.0.clone(),
        },
        system,
        vendors: config
            .models
            .iter()
            .map(|model| (model.offer.value.clone(), model.vendor))
            .collect(),
        windows: config
            .context_windows
            .iter()
            .map(|(model, window)| (model.as_str().to_owned(), *window))
            .collect(),
        tools: tools::specs()
            .into_iter()
            .chain([
                questions::spec(),
                plan::spec(),
                checklist::spec(),
                agent::spec(),
            ])
            .chain(mcp.tools().iter().map(mcp_spec))
            .collect(),
        skills,
        mcp,
    }
}

fn catalog(setup: &Setup) -> Catalog {
    Catalog {
        models: Vec::new(),
        commands: setup
            .skills
            .iter()
            .map(|skill| CommandOffer {
                name: skill.name.clone(),
                description: skill.description.clone(),
            })
            .collect(),
        subscription: None,
    }
}

fn open(
    config: &EngineConfig,
    session: &LiveSession,
    sink: &mut impl FnMut(LiveEvent),
) -> Option<Start> {
    let (mut file, restored) = match SessionFile::open(&config.sessions, session) {
        Ok(opened) => opened,
        Err(error) => {
            sink(refusal(&error).map_or_else(
                || LiveEvent::Chat(ChatEvent::Error(error.to_string())),
                LiveEvent::Refused,
            ));
            return None;
        }
    };
    let (toolbox, failures) = McpToolbox::start(&config.mcp, &config.cwd);
    for failure in &failures {
        sink(LiveEvent::Notice(failure_notice(failure)));
    }
    let setup = setup(config, Arc::new(toolbox));
    let model = chosen(config, &setup, &restored);
    if let Some(model) = model.as_ref().filter(|model| !restored.holds(model)) {
        if let Err(error) = file.append(&Record::from(model)) {
            sink(LiveEvent::Chat(ChatEvent::Error(error.to_string())));
        }
    }
    let mode = config.permission_mode.unwrap_or(PermissionMode::Manual);
    sink(LiveEvent::Chat(ChatEvent::Session(file.id.clone())));
    if let Some(model) = &model {
        sink(LiveEvent::Chat(ChatEvent::Model(model.id.clone())));
    }
    sink(LiveEvent::Chat(ChatEvent::PermissionMode(mode)));
    sink(LiveEvent::Chat(ChatEvent::ContextWindow(
        setup.window(model.as_ref()).tokens(),
    )));
    sink(LiveEvent::Ready(catalog(&setup)));
    Some(Start {
        setup,
        snapshots: Snapshots::new(file.snapshots.clone()),
        session: file,
        messages: restored.messages,
        prompts: restored.prompts,
        turn: None,
        model,
        effort: config.effort,
        mode,
    })
}

fn chosen(config: &EngineConfig, setup: &Setup, restored: &Restored) -> Option<Chosen> {
    let id = config
        .model
        .as_ref()
        .map(|model| model.as_str().to_owned())
        .or_else(|| restored.model.clone())?;
    let vendor = restored
        .vendor
        .filter(|_| restored.model.as_ref() == Some(&id))
        .unwrap_or_else(|| setup.vendor(&id));
    Some(Chosen { id, vendor })
}

pub(crate) fn spawn_engine(
    config: EngineConfig,
    session: &LiveSession,
    mut sink: impl FnMut(LiveEvent) + Send + 'static,
) -> EngineChat {
    let (orders, inbox) = channel();
    let running = Arc::new(AtomicBool::new(true));
    let alive = Arc::clone(&running);
    let session = session.clone();
    let inbound = orders.clone();
    let background = Background::default();
    let owned = background.clone();
    thread::spawn(move || {
        if let Some(start) = open(&config, &session, &mut sink) {
            Worker::new(start, sink, inbound, owned).run(&inbox);
        }
        alive.store(false, Ordering::SeqCst);
    });
    EngineChat {
        orders,
        running,
        background,
    }
}

/// # Errors
///
/// Fails when the conversation file cannot be read.
pub fn read_engine_transcript(
    sessions: &Path,
    cwd: &Path,
    resume: &ResumeId,
) -> io::Result<Vec<ChatEntry>> {
    let records = read_records(&session_path(sessions, resume))?;
    let workspace = Workspace {
        cwd: cwd.to_path_buf(),
        env: Vec::new(),
    };
    Ok(history::transcript(records, &workspace))
}
