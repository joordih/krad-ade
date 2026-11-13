use krad_chat::{AcpConfig, AcpWanted, BusVars, Runner};
use krad_domain::{AcpAgentId, AcpAgentStart, AcpModelId, AgentPreset, McpServerStart};
use krad_protocol::AgentRef;
use std::collections::HashMap;
use std::path::PathBuf;

use super::super::backend::{ChatLaunch, Choices, Unready};
use super::super::brief::{ChatBrief, ChatRole};
use super::super::{ChatPlace, ChatWork};
use super::{local_home, problem, Offered, Resolved, NO_CONFIG_FOLDER_TEXT, NO_PROGRAM_TEXT};
use crate::adapters::{kept_package, AdapterLaunch};

const INSTALLING_TEXT: &str = "Installing";
const INSTALLING_TAIL: &str = "for its first chat…";
const NO_NODE_TEXT: &str =
    "needs Node.js to install itself. Install Node.js (with npm), then send the message again.";
const FAILED_TEXT: &str = "could not install itself:";

fn adapter_runner(
    launch: AdapterLaunch,
    name: &str,
    place: &ChatPlace,
) -> Result<(Runner, PathBuf), Unready> {
    match launch {
        AdapterLaunch::Launch(program) => {
            let cwd = place.root.clone().or_else(local_home).unwrap_or_default();
            Ok((Runner::Native(program), cwd))
        }
        AdapterLaunch::Waiting => Err(Unready::Waiting),
        AdapterLaunch::Installing => Err(Unready::Problem(format!(
            "{INSTALLING_TEXT} {name} {INSTALLING_TAIL}"
        ))),
        AdapterLaunch::NoNode => Err(Unready::Problem(format!("{name} {NO_NODE_TEXT}"))),
        AdapterLaunch::Failed(reason) => {
            Err(Unready::Problem(format!("{name} {FAILED_TEXT} {reason}")))
        }
        AdapterLaunch::NoConfigFolder => Err(Unready::Problem(NO_CONFIG_FOLDER_TEXT.to_owned())),
    }
}

pub(in crate::chats) fn resolve_acp(
    (chat, agent, place): (&AgentRef, AcpAgentId, &ChatPlace),
    (brief, choices): (&ChatBrief, &Choices),
    mcp: Vec<McpServerStart>,
    work: &mut ChatWork<'_>,
) -> Result<Resolved, Unready> {
    let preset = AgentPreset::Acp(agent);
    let name = work.facts.settings.acp.harness_name(preset);
    let agent = work
        .facts
        .settings
        .acp
        .agent(agent)
        .ok_or_else(|| problem(&name, NO_PROGRAM_TEXT))?;
    let remote = work.facts.site(chat, place)?.remote.is_some();
    let (runner, cwd) = match kept_package(agent.id, &place.link, place.root.as_deref()) {
        Some(package) => {
            let launch = work.adapters.start(&agent, package, work.jobs, work.outbox);
            adapter_runner(launch, &name, place)?
        }
        None => work
            .facts
            .program_runner(chat, (preset, agent.program.clone()), place)?,
    };
    let off_bus = brief.role == ChatRole::Digest || (runner.on_ssh() && !remote);
    let bus_vars = if off_bus {
        Vec::new()
    } else {
        work.facts.bus_vars(chat, place, runner.in_wsl())
    };
    let offered = work.facts.offered(preset, place, choices);
    let config = AcpConfig {
        runner,
        agent: AcpAgentStart {
            agent,
            secrets: HashMap::new(),
        },
        cwd,
        mcp,
        bus_vars: BusVars(bus_vars),
        buffers: work.facts.buffers.of(&chat.environment),
        model: offered
            .model()
            .map(|model| AcpModelId(model.as_str().to_owned())),
        wanted: AcpWanted::default(),
    };
    let model_dropped = matches!(offered, Offered::Dropped(_));
    Ok(Resolved {
        launch: ChatLaunch::Acp(Box::new(config)),
        notices: offered.notice().into_iter().collect(),
        model_dropped,
    })
}
