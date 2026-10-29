mod chat;
mod lead;
mod requests;

use futures::channel::mpsc::{channel, Receiver};
use futures::channel::oneshot::Sender as ReplySender;
use futures::StreamExt as _;
use gpui_kit::Context;
use krad_agents::{
    lead_refusal, notice_text, serve, terminal_env, Caller, Endpoint, Incoming, LeadHandle,
    Request, Response, TerminalHandle, CLI_PROGRAM,
};
use krad_chat::ToolAsk;
use krad_domain::{claude_hook_settings, parse_wsl_path, AgentState, EnvironmentName, TerminalId};
use krad_session::PtyInput;
use krad_store::config_dir;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::agent_hooks::{HookReport, PendingPrompt};
use super::terminal_actions::paste_bytes;
use super::{EnvironmentShell, LiveEnv};

const BUS_CAPACITY: usize = 64;
const PATH_ENV: &str = "PATH";
const CLAUDE_SETTINGS_FILE: &str = "claude-hooks.json";
const NOTICE_ENTER_DELAY: Duration = Duration::from_millis(500);
pub(super) const ENTER: &[u8] = b"\r";
const KEY_STEP_DELAY: Duration = Duration::from_secs(1);
const ENTER_RETRY_DELAY: Duration = Duration::from_secs(2);
const ENTER_RETRIES: u8 = 3;
const WSLENV_VAR: &str = "WSLENV";
const WSLENV_SEPARATOR: &str = ":";

pub(super) const BUS_HINT: &str = "You run inside Krad ADE next to other coding agents. Run 'krad-ade-agents help' to see them, message them and wait for them.";
const NO_SUCH_TERMINAL: &str = "No terminal with that id in your environment";

pub(super) struct AgentBus {
    endpoint: Endpoint,
    cli_dir: Option<PathBuf>,
    inbox_waiters: Vec<(TerminalHandle, ReplySender<Response>)>,
    settle_waiters: Vec<(TerminalHandle, ReplySender<Response>)>,
    enters_due: Vec<PendingEnter>,
    keys_due: Vec<PendingKeys>,
    pub(super) pending_prompts: Vec<PendingPrompt>,
    pub(super) claude_settings: Option<PathBuf>,
}

struct PendingKeys {
    target: TerminalHandle,
    keys: Vec<u8>,
    due: Instant,
}

struct PendingEnter {
    target: TerminalHandle,
    due: Instant,
    retries_left: u8,
    is_retry: bool,
}

impl PendingEnter {
    fn new(target: TerminalHandle, now: Instant) -> Self {
        Self {
            target,
            due: now + NOTICE_ENTER_DELAY,
            retries_left: ENTER_RETRIES,
            is_retry: false,
        }
    }

    fn retry(self, now: Instant) -> Option<Self> {
        Some(Self {
            due: now + ENTER_RETRY_DELAY,
            retries_left: self.retries_left.checked_sub(1)?,
            is_retry: true,
            target: self.target,
        })
    }
}

impl AgentBus {
    fn is_starting(&self, target: &TerminalHandle) -> bool {
        self.pending_prompts
            .iter()
            .any(|prompt| prompt.target == *target)
            || self.enters_due.iter().any(|enter| enter.target == *target)
    }
}

enum Settlement {
    Missing,
    Working,
    Settled(String),
}

pub(super) fn cli_name(in_wsl: bool) -> &'static str {
    if in_wsl {
        CLI_WSL_NAME
    } else {
        CLI_PROGRAM
    }
}

const CLI_WSL_NAME: &str = "krad-ade-agents.exe";

fn write_claude_settings() -> Option<PathBuf> {
    let path = config_dir()?.join(CLAUDE_SETTINGS_FILE);
    let folder = path.parent()?;
    std::fs::create_dir_all(folder).ok()?;
    std::fs::write(&path, claude_hook_settings(CLI_PROGRAM)).ok()?;
    Some(path)
}

fn handle(env: &EnvironmentName, id: TerminalId) -> TerminalHandle {
    TerminalHandle {
        environment: env.as_str().to_owned(),
        terminal: id.as_u64(),
    }
}

impl EnvironmentShell {
    pub(super) fn start_agent_bus(&mut self, cx: &mut Context<Self>) {
        let (requests, incoming) = channel(BUS_CAPACITY);
        let Ok(endpoint) = serve(requests) else {
            return;
        };
        let cli_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from));
        self.agent_bus = Some(AgentBus {
            endpoint,
            cli_dir,
            inbox_waiters: Vec::new(),
            settle_waiters: Vec::new(),
            enters_due: Vec::new(),
            keys_due: Vec::new(),
            pending_prompts: Vec::new(),
            claude_settings: write_claude_settings(),
        });
        cx.spawn(async move |this, cx| {
            let mut incoming: Receiver<Incoming> = incoming;
            while let Some(request) = incoming.next().await {
                if this
                    .update(cx, |shell, cx| shell.answer(request, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn bus_vars(&self, env: &EnvironmentName, id: TerminalId) -> Vec<(String, String)> {
        let Some(bus) = &self.agent_bus else {
            return Vec::new();
        };
        let mut vars = Self::caller_vars(bus, &Caller::Terminal(handle(env, id)));
        if self.terminal_in_wsl(env, id) {
            let shared: Vec<String> = vars
                .iter()
                .filter(|(name, _)| name != PATH_ENV)
                .map(|(name, _)| name.clone())
                .collect();
            vars.push((WSLENV_VAR.to_owned(), shared.join(WSLENV_SEPARATOR)));
        }
        vars
    }

    fn caller_vars(bus: &AgentBus, caller: &Caller) -> Vec<(String, String)> {
        let mut vars = terminal_env(&bus.endpoint, caller);
        let mut search: Vec<PathBuf> = bus.cli_dir.iter().cloned().collect();
        search.extend(
            std::env::var_os(PATH_ENV)
                .iter()
                .flat_map(std::env::split_paths),
        );
        if let Ok(path) = std::env::join_paths(search) {
            vars.push((PATH_ENV.to_owned(), path.to_string_lossy().into_owned()));
        }
        vars
    }

    fn terminal_in_wsl(&self, env: &EnvironmentName, id: TerminalId) -> bool {
        self.envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .is_some_and(|group| parse_wsl_path(&group.root).is_some())
    }

    fn answer(&mut self, incoming: Incoming, cx: &mut Context<Self>) {
        let Incoming {
            from,
            request,
            reply,
        } = incoming;
        let from = self.as_lead(from, &request);
        if let Some(refusal) = lead_refusal(&from, &request) {
            reply.send(refusal).unwrap_or_default();
            return;
        }
        match from {
            Caller::Terminal(from) => self.answer_terminal(from, request, reply, cx),
            Caller::Chat(chat) => self.answer_chat(&chat, request, reply, cx),
            Caller::Lead(lead) => self.answer_lead(&lead, request, reply, cx),
        }
        cx.notify();
    }

    fn as_lead(&self, from: Caller, request: &Request) -> Caller {
        let Caller::Terminal(terminal) = &from else {
            return from;
        };
        if !request.directs_workers() {
            return from;
        }
        let session = self
            .envs
            .get(&EnvironmentName::new(terminal.environment.clone()))
            .and_then(|live| live.data.session_led_by(TerminalId(terminal.terminal)));
        match session {
            Some(session) => Caller::Lead(LeadHandle {
                environment: terminal.environment.clone(),
                session: session.as_u64(),
            }),
            None => from,
        }
    }

    fn answer_terminal(
        &mut self,
        from: TerminalHandle,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let response = match request {
            Request::List => Some(Response::Agents(
                self.agent_infos(&from.environment, Some(from.terminal)),
            )),
            Request::Send { to, body } => Some(self.post(&from, to, body)),
            Request::Check { wait_secs } => {
                let messages = self.take_mail(&from);
                if messages.is_empty() && wait_secs.is_some() {
                    if let Some(bus) = &mut self.agent_bus {
                        bus.inbox_waiters.push((from, reply));
                    }
                    return;
                }
                Some(Response::Messages(messages))
            }
            Request::Wait { target, .. } => {
                self.wait_for(&from.environment, target, reply);
                return;
            }
            Request::Read { target, lines } => {
                Some(self.screen_tail(&from.environment, target, lines))
            }
            Request::Hook {
                harness,
                event,
                tool,
                detail,
                resume,
            } => {
                let report = HookReport {
                    harness: &harness,
                    event: &event,
                    tool: tool.as_deref(),
                    detail,
                    resume: resume.as_deref(),
                };
                Some(self.apply_hook(&from, report))
            }
            Request::Status(status) => Some(self.apply_status(&from, status)),
            Request::Ask {
                tool,
                input,
                suggestions,
            } => {
                let ask = ToolAsk::held(tool.clone(), input, suggestions);
                self.hold_ask(&from, ask, tool, reply);
                cx.notify();
                return;
            }
            Request::Spawn {
                agent,
                worktree,
                task,
            } => {
                self.spawn_worker(&from, &agent, worktree, &task, reply, cx);
                return;
            }
        };
        if let Some(response) = response {
            reply.send(response).unwrap_or_default();
        }
    }

    fn wait_for(&mut self, environment: &str, target: u64, reply: ReplySender<Response>) {
        let target = TerminalHandle {
            environment: environment.to_owned(),
            terminal: target,
        };
        let starting = self
            .agent_bus
            .as_ref()
            .is_some_and(|bus| bus.is_starting(&target));
        match self.settled_state(&target) {
            Settlement::Settled(state) if !starting => reply
                .send(Response::Settled {
                    target: target.terminal,
                    state,
                })
                .unwrap_or_default(),
            Settlement::Missing => reply
                .send(Response::Error(NO_SUCH_TERMINAL.to_owned()))
                .unwrap_or_default(),
            Settlement::Settled(_) | Settlement::Working => {
                if let Some(bus) = &mut self.agent_bus {
                    bus.settle_waiters.push((target, reply));
                }
            }
        }
    }

    pub(super) fn service_agent_bus(&mut self, now: Instant) {
        let Some(mut bus) = self.agent_bus.take() else {
            return;
        };
        bus.settle_waiters.retain(|(_, reply)| !reply.is_canceled());
        let waiters = std::mem::take(&mut bus.settle_waiters);
        for (target, reply) in waiters {
            match self.settled_state(&target) {
                Settlement::Settled(_) if bus.is_starting(&target) => {
                    bus.settle_waiters.push((target, reply));
                }
                Settlement::Settled(state) => reply
                    .send(Response::Settled {
                        target: target.terminal,
                        state,
                    })
                    .unwrap_or_default(),
                Settlement::Working => bus.settle_waiters.push((target, reply)),
                Settlement::Missing => reply
                    .send(Response::Error(NO_SUCH_TERMINAL.to_owned()))
                    .unwrap_or_default(),
            }
        }
        let (keys_now, keys_later): (Vec<_>, Vec<_>) =
            bus.keys_due.drain(..).partition(|keys| keys.due <= now);
        bus.keys_due = keys_later;
        for pending in keys_now {
            self.type_into(&pending.target, pending.keys);
        }
        let (due, later): (Vec<_>, Vec<_>) =
            bus.enters_due.drain(..).partition(|enter| enter.due <= now);
        bus.enters_due = later;
        for enter in due {
            if enter.is_retry && !self.submit_unconfirmed(&enter.target) {
                continue;
            }
            self.type_into(&enter.target, ENTER.to_vec());
            if self.submit_unconfirmed(&enter.target) {
                bus.enters_due.extend(enter.retry(now));
            }
        }
        let (ready, waiting): (Vec<_>, Vec<_>) = bus
            .pending_prompts
            .drain(..)
            .partition(|prompt| prompt.due <= now);
        bus.pending_prompts = waiting;
        for prompt in ready {
            self.paste_text_into(&prompt.target, &prompt.text);
            bus.enters_due.push(PendingEnter::new(prompt.target, now));
        }
        for (env, live) in &mut self.envs {
            let settled: Vec<TerminalId> = live
                .data
                .nodes
                .iter()
                .filter(|node| node.state != AgentState::Working)
                .filter(|node| {
                    live.sessions
                        .get(&node.id)
                        .is_some_and(|session| session.is_agent && session.state.is_running())
                })
                .map(|node| node.id)
                .collect();
            for id in settled {
                let target = handle(env, id);
                let blocked = bus
                    .inbox_waiters
                    .iter()
                    .any(|(waiter, _)| *waiter == target);
                if blocked {
                    continue;
                }
                let in_wsl = live
                    .data
                    .group_of(id)
                    .is_some_and(|group| parse_wsl_path(&group.root).is_some());
                if let Some(count) = live.mailbox.notify(id.as_u64()) {
                    if let Some(session) = live.sessions.get(&id) {
                        let notice = notice_text(cli_name(in_wsl), count).into_bytes();
                        session
                            .writer_tx
                            .send(PtyInput::Bytes(notice))
                            .unwrap_or_default();
                    }
                    bus.enters_due.push(PendingEnter::new(target, now));
                }
            }
            Self::notice_free_chats(env, live, &bus);
        }
        self.agent_bus = Some(bus);
    }

    fn notice_free_chats(env: &EnvironmentName, live: &mut LiveEnv, bus: &AgentBus) {
        let free: Vec<TerminalId> = live
            .chats
            .iter()
            .filter(|(_, chat)| chat.takes_bus_notice())
            .map(|(id, _)| *id)
            .collect();
        for id in free {
            let target = handle(env, id);
            if bus
                .inbox_waiters
                .iter()
                .any(|(waiter, _)| *waiter == target)
            {
                continue;
            }
            let Some(count) = live.mailbox.notify(id.as_u64()) else {
                continue;
            };
            let in_wsl = live
                .data
                .group_of(id)
                .is_some_and(|group| parse_wsl_path(&group.root).is_some());
            if let Some(chat) = live.chats.get_mut(&id) {
                chat.queue_bus_notice(notice_text(cli_name(in_wsl), count));
                live.noticed_chats.push(id);
            }
        }
    }

    pub(super) fn submit_text_into(&mut self, env: &EnvironmentName, id: TerminalId, text: &str) {
        let target = handle(env, id);
        self.paste_text_into(&target, text);
        match &mut self.agent_bus {
            Some(bus) => bus
                .enters_due
                .push(PendingEnter::new(target, Instant::now())),
            None => self.type_into(&target, ENTER.to_vec()),
        }
    }

    fn submit_unconfirmed(&self, target: &TerminalHandle) -> bool {
        let Some(live) = self
            .envs
            .get(&EnvironmentName::new(target.environment.clone()))
        else {
            return false;
        };
        let id = TerminalId(target.terminal);
        let hooked = live
            .sessions
            .get(&id)
            .is_some_and(|session| session.hooked && session.state.is_running());
        hooked
            && live
                .data
                .nodes
                .iter()
                .any(|node| node.id == id && node.state != AgentState::Working)
    }

    fn paste_text_into(&self, target: &TerminalHandle, text: &str) {
        let bracketed = self
            .envs
            .get(&EnvironmentName::new(target.environment.clone()))
            .and_then(|live| live.sessions.get(&TerminalId(target.terminal)))
            .is_some_and(|session| session.engine.bracketed_paste());
        self.type_into(target, paste_bytes(text, bracketed));
    }

    pub(super) fn type_in_steps(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        steps: Vec<Vec<u8>>,
    ) {
        let target = handle(env, id);
        let mut steps = steps.into_iter();
        if let Some(first) = steps.next() {
            self.type_into(&target, first);
        }
        let Some(bus) = &mut self.agent_bus else {
            return;
        };
        let now = Instant::now();
        let mut due = now;
        for keys in steps {
            due += KEY_STEP_DELAY;
            bus.keys_due.push(PendingKeys {
                target: target.clone(),
                keys,
                due,
            });
        }
    }

    pub(super) fn type_into_terminal(&self, env: &EnvironmentName, id: TerminalId, bytes: Vec<u8>) {
        self.type_into(&handle(env, id), bytes);
    }

    fn type_into(&self, target: &TerminalHandle, bytes: Vec<u8>) {
        if let Some(session) = self
            .envs
            .get(&EnvironmentName::new(target.environment.clone()))
            .and_then(|live| live.sessions.get(&TerminalId(target.terminal)))
        {
            session
                .writer_tx
                .send(PtyInput::Bytes(bytes))
                .unwrap_or_default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unconfirmed_enter_is_retried_a_bounded_number_of_times() {
        let now = Instant::now();
        let target = TerminalHandle {
            environment: "rag".to_owned(),
            terminal: 7,
        };
        let mut enter = Some(PendingEnter::new(target, now));
        let mut presses = 0;
        while let Some(pending) = enter {
            presses += 1;
            enter = pending.retry(now);
            assert!(enter.as_ref().is_none_or(|next| next.is_retry));
        }
        assert_eq!(presses, 1 + usize::from(ENTER_RETRIES));
    }
}
