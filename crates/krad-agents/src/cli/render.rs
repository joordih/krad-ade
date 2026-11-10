use super::memory::{render_chat_opened, render_entry, render_hits, render_recall, render_shared};
use super::status::AgentStates;
use super::variants::render_runs;
use super::verb::Verb;
use crate::extension_tools::render_catalog;
use crate::protocol::{
    bus_name, AgentInfo, AutomationInfo, ChangeSet, Closed, ClosedTarget, KraddieInfo,
    KraddieMemoryInfo, Message, RecallView, Request, Response, SessionInfo, WorktreeOutcome,
};
use krad_domain::ChangeKind;

const NO_AGENTS: &str = "No agents";
const NO_SESSIONS: &str = "No sessions";
const NO_MESSAGES: &str = "No new messages";
const NO_CHANGES: &str = "No changes";
const NOTHING_CLOSED: &str = "Nothing to close";
const NO_KRADDIES: &str = "No Kraddies";
const NO_AUTOMATIONS: &str = "No automations";
const TRIGGER_SEPARATOR: &str = ", ";
const RUNS_MARKER: &str = "  runs: ";
const NO_MEMORY: &str = "No keepsakes yet";
const MEMORY_METER_SUFFIX: &str = "kept";
const KRADDIE_OFF_MARKER: &str = "  (off)";
const NEXT_RUN_MARKER: &str = "  next: ";
const LAST_RUN_MARKER: &str = "  last: ";
const TIMED_OUT: &str = "Timed out";
const SELF_MARKER: &str = "  (you)";
const CURRENT_MARKER: &str = "  (current)";
const AGENT_MARKER: &str = "  agent: ";

pub(super) fn render(verb: Verb, request: &Request, response: &Response) -> String {
    match response {
        Response::Agents(agents) if verb == Verb::Status => AgentStates::sort(agents).render(),
        Response::Agents(agents) if agents.is_empty() => NO_AGENTS.to_owned(),
        Response::Agents(agents) => lines(agents.iter().map(render_agent)),
        Response::Sent { id } => match request {
            Request::Send { to, .. } => format!("Sent message {id} to terminal {to}"),
            _ => format!("Sent message {id}"),
        },
        Response::Messages(messages) if messages.is_empty() => NO_MESSAGES.to_owned(),
        Response::Messages(messages) => lines(messages.iter().map(render_message)),
        Response::Settled { target, state } => format!("Terminal {target} is {state}"),
        Response::TimedOut => TIMED_OUT.to_owned(),
        Response::Screen(screen) => screen.clone(),
        Response::Noted | Response::Decided(_) | Response::Answered(_) => String::new(),
        Response::Spawned { terminal } => {
            format!("Started worker {terminal} in a session of its own")
        }
        Response::Submitted { terminal } => format!("Typed the message into terminal {terminal}"),
        Response::Code(answer) => answer.render(),
        Response::Tools(catalog) => render_catalog(catalog),
        Response::ExtensionTool(answer) => answer.text.clone(),
        Response::Hello(info) => format!("Agents API {} (build {})", info.api_version, info.build),
        Response::Sessions(sessions) if sessions.is_empty() => NO_SESSIONS.to_owned(),
        Response::Sessions(sessions) => lines(sessions.iter().map(render_session)),
        Response::SessionCreated { session } => format!("Created session {session}"),
        Response::SessionClosed { session } => format!("Closed session {session}"),
        Response::WorktreeCreated(worktree) => format!(
            "Created worktree {} at {}",
            worktree.group,
            worktree.path.display()
        ),
        Response::Opened { path } => format!("Opened {}", path.display()),
        Response::Changes(changes) if verb == Verb::Diff => changes.patch.clone(),
        Response::Changes(changes) => render_changes(changes),
        Response::Closed(closed) if closed.is_empty() => NOTHING_CLOSED.to_owned(),
        Response::Closed(closed) => lines(closed.iter().map(render_closed)),
        Response::Kraddies(kraddies) if kraddies.is_empty() => NO_KRADDIES.to_owned(),
        Response::Kraddies(kraddies) => lines(kraddies.iter().map(render_kraddie)),
        Response::KraddieStarted(started) => {
            format!("Woke Kraddie {} (run {})", started.kraddie, started.run)
        }
        Response::Automations(automations) if automations.is_empty() => NO_AUTOMATIONS.to_owned(),
        Response::Automations(automations) => lines(automations.iter().map(render_automation)),
        Response::AutomationStarted(started) => format!(
            "Started automation {}/{} (run {})",
            started.kraddie, started.automation, started.run
        ),
        Response::AutomationSwitched(automation) => format!(
            "Automation {}/{} is {}",
            automation.kraddie,
            automation.name,
            automation.switch.display_name().to_lowercase()
        ),
        Response::KraddieMemory(memory) => render_memory(memory),
        Response::KraddieRecall(recall) => match request {
            Request::KraddieRecall { view, .. } => render_recall(recall, *view),
            _ => render_recall(recall, RecallView::default()),
        },
        Response::KraddieChatOpened(opened) => render_chat_opened(opened),
        Response::KraddieShared(shared) => render_shared(shared),
        Response::MemoryHits(hits) => render_hits(hits),
        Response::MemoryEntry(entry) => render_entry(entry),
        Response::VariantsStarted { run } => format!("Started variant run {run}"),
        Response::Variants(runs) => render_runs(runs, std::time::SystemTime::now()),
        Response::VariantPicked { run, variant } => {
            let variant = krad_domain::VariantId(*variant);
            format!("Variant {variant} won run {run}; the others were discarded")
        }
        Response::AgentSnapshot(_) | Response::AgentEvent(_) | Response::Heartbeat(_) => {
            super::watch::watch_line(response, false).unwrap_or_default()
        }
        Response::Error(error) => error.message.clone(),
    }
}

fn render_closed(closed: &Closed) -> String {
    let what = match closed.target {
        ClosedTarget::Agent(terminal) => format!("Closed worker {terminal}"),
        ClosedTarget::Session(session) => format!("Closed session {session}"),
    };
    let Some(worktree) = &closed.worktree else {
        return what;
    };
    let path = worktree.path.display();
    match &worktree.outcome {
        WorktreeOutcome::Kept => format!("{what}; its worktree stays at {path}"),
        WorktreeOutcome::Removed => format!("{what}; removed its worktree {path}"),
        WorktreeOutcome::Refused(error) => format!(
            "{what}; kept its worktree at {path} ({}): {}",
            error.code, error.message
        ),
    }
}

pub(super) fn render_agent(agent: &AgentInfo) -> String {
    let group = agent
        .group
        .as_ref()
        .map_or_default(|group| format!("  group: {group}"));
    let you = if agent.is_self { SELF_MARKER } else { "" };
    let harness = agent
        .harness
        .and_then(bus_name)
        .map_or_default(|name| format!("{AGENT_MARKER}{name}"));
    format!(
        "{}  {}  [{}]{harness}{group}{you}",
        agent.terminal, agent.title, agent.state
    )
}

fn render_session(session: &SessionInfo) -> String {
    let group = session
        .group
        .as_ref()
        .map_or_default(|group| format!("  group: {group}"));
    let current = if session.is_current {
        CURRENT_MARKER
    } else {
        ""
    };
    let terminals = session
        .terminals
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}  {}  [{terminals}]{group}{current}",
        session.session, session.name
    )
}

fn render_memory(memory: &KraddieMemoryInfo) -> String {
    let meter = format!(
        "{} of {} {MEMORY_METER_SUFFIX}",
        memory.lines.len(),
        memory.limit
    );
    let listed = if memory.lines.is_empty() {
        NO_MEMORY.to_owned()
    } else {
        lines(
            memory
                .lines
                .iter()
                .enumerate()
                .map(|(index, line)| format!("{}  {line}", index + 1)),
        )
    };
    lines([listed, meter].into_iter())
}

fn render_kraddie(kraddie: &KraddieInfo) -> String {
    let marker = |label: &str, value: Option<&String>| {
        value.map_or_default(|value| format!("{label}{value}"))
    };
    let off = if kraddie.switch.is_on() {
        ""
    } else {
        KRADDIE_OFF_MARKER
    };
    let schedule = kraddie
        .schedule
        .as_ref()
        .map_or_default(|schedule| format!("  [{schedule}]"));
    format!(
        "{}  {}  {}{schedule}{}{}{off}",
        kraddie.id,
        kraddie.name,
        kraddie.harness,
        marker(NEXT_RUN_MARKER, kraddie.next_run.as_ref()),
        marker(LAST_RUN_MARKER, kraddie.last_outcome.as_ref()),
    )
}

fn render_automation(automation: &AutomationInfo) -> String {
    let marker = |label: &str, value: Option<&String>| {
        value.map_or_default(|value| format!("{label}{value}"))
    };
    let off = if automation.switch.is_on() {
        ""
    } else {
        KRADDIE_OFF_MARKER
    };
    let triggers = if automation.triggers.is_empty() {
        String::new()
    } else {
        format!("  [{}]", automation.triggers.join(TRIGGER_SEPARATOR))
    };
    format!(
        "{}/{}{triggers}{}{}{RUNS_MARKER}{}{off}",
        automation.kraddie,
        automation.name,
        marker(NEXT_RUN_MARKER, automation.next_run.as_ref()),
        marker(LAST_RUN_MARKER, automation.last_outcome.as_ref()),
        automation.runs,
    )
}

fn render_changes(changes: &ChangeSet) -> String {
    if changes.files.is_empty() {
        return NO_CHANGES.to_owned();
    }
    lines(
        changes
            .files
            .iter()
            .map(|file| format!("{} {}", change_mark(file.kind), file.path.display())),
    )
}

const fn change_mark(kind: ChangeKind) -> char {
    match kind {
        ChangeKind::Modified => 'M',
        ChangeKind::Added => 'A',
        ChangeKind::Deleted => 'D',
        ChangeKind::Renamed => 'R',
        ChangeKind::Untracked => '?',
        ChangeKind::Conflicted => 'U',
    }
}

fn render_message(message: &Message) -> String {
    format!(
        "#{} from {} ({}): {}",
        message.id, message.from, message.from_title, message.body
    )
}

fn lines(rendered: impl Iterator<Item = String>) -> String {
    rendered.collect::<Vec<_>>().join("\n")
}
