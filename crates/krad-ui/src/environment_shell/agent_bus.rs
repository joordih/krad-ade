mod askpass;
mod chat;
mod close;
mod discovery;
mod feed;
mod lead;
mod requests;
mod service;
mod typing;
mod workspace;

pub(in crate::environment_shell) use askpass::AskpassPlan;
pub(super) use workspace::target_group;

use futures::StreamExt as _;
use gpui_kit::Context;
use krad_agents::{Caller, ErrorCode, Request, Response, TerminalHandle, TerminalState};
use krad_domain::{parse_wsl_path, EnvironmentName, Host, TerminalId};
use krad_runtime::{local_caller_vars, remote_caller_vars, BusReply, Delegated};
use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::agent_hooks::{HookReport, PendingPrompt};
use super::language_servers::CodeCaller;
use super::EnvironmentShell;

const NOTICE_ENTER_DELAY: Duration = Duration::from_millis(500);
pub(super) const ENTER: &[u8] = b"\r";
const KEY_STEP_DELAY: Duration = Duration::from_secs(1);
const ENTER_RETRY_DELAY: Duration = Duration::from_secs(2);
const ENTER_RETRIES: u8 = 3;

const NO_SUCH_TERMINAL: &str = "No terminal with that id in your environment";
const ONLY_LEADS_CLOSE: &str = "Only a session's lead can close workers and sessions";

const ANSWERED_BY_RUNTIME: &str = "The runtime answers this request before it reaches the app";

#[derive(Default)]
pub(super) struct BusWork {
    settle_waiters: Vec<(TerminalHandle, BusReply)>,
    enters_due: Vec<PendingEnter>,
    keys_due: Vec<PendingKeys>,
    pub(super) pending_prompts: Vec<PendingPrompt>,
    pub(super) remote_unavailable_told: HashSet<Host>,
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

impl BusWork {
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
    Settled(TerminalState),
}

fn answered_by_runtime() -> Response {
    Response::error(ErrorCode::Unsupported, ANSWERED_BY_RUNTIME)
}

fn handle(env: &EnvironmentName, id: TerminalId) -> TerminalHandle {
    TerminalHandle {
        environment: env.as_str().to_owned(),
        terminal: id.as_u64(),
    }
}

impl EnvironmentShell {
    pub(super) fn start_agent_bus(&mut self, cx: &mut Context<Self>) {
        if self.runtime.handle.bus().is_none() {
            return;
        }
        self.sync_api_file();
        let mut delegated = self.runtime.handle.delegate_bus();
        cx.spawn(async move |this, cx| {
            while let Some(request) = delegated.next().await {
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
        let Some(bus) = self.runtime.handle.bus() else {
            return Vec::new();
        };
        if self.cloud_placement_of(env, id).is_some() {
            return self.cloud_bus_vars(env, id);
        }
        if self
            .envs
            .get(env)
            .is_some_and(|live| !live.data.host.is_local())
        {
            return self.remote_bus(env).map_or_default(|remote| {
                remote_caller_vars(remote, &Caller::Terminal(handle(env, id)))
            });
        }
        let caller = Caller::Terminal(handle(env, id));
        local_caller_vars(bus, &caller, self.terminal_in_wsl(env, id))
    }

    fn terminal_in_wsl(&self, env: &EnvironmentName, id: TerminalId) -> bool {
        self.envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .is_some_and(|group| parse_wsl_path(&group.root).is_some())
    }

    fn answer(&mut self, delegated: Delegated, cx: &mut Context<Self>) {
        let Delegated {
            from,
            request,
            reply,
        } = delegated;
        if let Some(refusal) = self.memory_gate(&from, &request) {
            reply.send(Response::Error(refusal)).unwrap_or_default();
            return;
        }
        match from {
            Caller::Terminal(from) => self.answer_terminal(&from, request, reply, cx),
            Caller::Lead(lead) => self.answer_lead(&lead, request, reply, cx),
            Caller::Git(_) => reply.send(answered_by_runtime()).unwrap_or_default(),
            Caller::Client => reply.send(feed::client_refusal()).unwrap_or_default(),
        }
        cx.notify();
    }

    fn answer_group_request(
        &mut self,
        from: &TerminalHandle,
        request: Request,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        match request {
            request @ (Request::Kraddies
            | Request::RunKraddie { .. }
            | Request::Automations
            | Request::RunAutomation { .. }
            | Request::SwitchAutomation { .. }
            | Request::RememberKraddie { .. }
            | Request::KraddieMemory { .. }) => {
                self.answer_kraddies(request, reply, cx);
            }
            request @ (Request::KraddieRecall { .. }
            | Request::NewKraddieChat { .. }
            | Request::ShareKraddieMemory { .. }
            | Request::MemorySearch { .. }
            | Request::MemoryRead { .. }) => {
                let chat = Some(TerminalId(from.terminal));
                self.answer_memory_call(&from.environment, chat, request, reply, cx);
            }
            request @ (Request::RunVariants(_)
            | Request::Variants { .. }
            | Request::PickVariant { .. }) => {
                let group = self.caller_group(from);
                self.answer_variants(&from.environment, group, request, reply, cx);
            }
            request => {
                let group = self.caller_group(from);
                self.answer_workspace(&from.environment, group, request, reply, cx);
            }
        }
    }

    fn answer_terminal(
        &mut self,
        from: &TerminalHandle,
        request: Request,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        let caller = CodeCaller::Terminal(TerminalId(from.terminal));
        let response = match request {
            Request::Code(code) => {
                self.answer_code(&from.environment, &caller, code, reply, cx);
                return;
            }
            Request::List
            | Request::ListAll
            | Request::Check { .. }
            | Request::Status(_)
            | Request::Ask { .. }
            | Request::CreateWorktree { .. }
            | Request::Changes { .. } => Some(answered_by_runtime()),
            Request::Close(_) => Some(Response::error(ErrorCode::Forbidden, ONLY_LEADS_CLOSE)),
            Request::Send { to, body } => {
                self.post(from, to, body, reply);
                return;
            }
            Request::Wait { target, .. } => {
                self.wait_for(&from.environment, target, reply);
                return;
            }
            Request::Read { target, lines } => {
                let tail = self.screen_tail(&from.environment, target, lines);
                Self::reply_tail(tail, reply, cx);
                return;
            }
            Request::Hook {
                harness,
                event,
                tool,
                detail,
                resume,
                mode,
            } => {
                let report = HookReport {
                    harness,
                    event: &event,
                    tool: tool.as_deref(),
                    detail,
                    resume: resume.as_deref(),
                    mode,
                };
                Some(self.apply_live_hook(from, report))
            }
            Request::Spawn {
                agent,
                worktree,
                task,
            } => {
                self.spawn_worker(from, agent, worktree, &task, reply, cx);
                return;
            }
            Request::Askpass { .. } => Some(askpass::git_only()),
            Request::Tools => Some(Response::Tools(self.tool_catalog())),
            Request::SubscribeAgents(_) => Some(feed::streamed_by_the_bus()),
            request @ Request::ExtensionTool { .. } => {
                self.answer_extension_tool(&from.environment, &caller, request, reply, cx);
                return;
            }
            request @ (Request::Kraddies
            | Request::RunKraddie { .. }
            | Request::Automations
            | Request::RunAutomation { .. }
            | Request::SwitchAutomation { .. }
            | Request::RememberKraddie { .. }
            | Request::KraddieMemory { .. }
            | Request::KraddieRecall { .. }
            | Request::NewKraddieChat { .. }
            | Request::ShareKraddieMemory { .. }
            | Request::MemorySearch { .. }
            | Request::MemoryRead { .. }
            | Request::Hello
            | Request::Sessions
            | Request::CreateSession { .. }
            | Request::CloseSession { .. }
            | Request::OpenFile { .. }
            | Request::RunVariants(_)
            | Request::Variants { .. }
            | Request::PickVariant { .. }) => {
                self.answer_group_request(from, request, reply, cx);
                return;
            }
        };
        if let Some(response) = response {
            reply.send(response).unwrap_or_default();
        }
    }

    fn wait_for(&mut self, environment: &str, target: u64, reply: BusReply) {
        let target = TerminalHandle {
            environment: environment.to_owned(),
            terminal: target,
        };
        let starting = self.bus_work.is_starting(&target);
        match self.settled_state(&target) {
            Settlement::Settled(state) if !starting => reply
                .send(Response::Settled {
                    target: target.terminal,
                    state,
                })
                .unwrap_or_default(),
            Settlement::Missing => reply
                .send(Response::error(ErrorCode::NotFound, NO_SUCH_TERMINAL))
                .unwrap_or_default(),
            Settlement::Settled(_) | Settlement::Working => {
                self.bus_work.settle_waiters.push((target, reply));
            }
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
