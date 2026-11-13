use gpui_kit::Context;
use krad_agents::{ErrorCode, Response, TerminalHandle};
use krad_domain::{
    code_tools_launch_args, hook_launch_args, launch_with_prompt, quote_for_shell,
    sanitize_worktree_name, worker_brief, AgentPreset, CloudPlacement, EnvironmentName,
    ModelDelivery, ModelId, PermissionMode, SessionId, TerminalGroup, TerminalId, ToastKind,
    WorkerSpawn,
};
use krad_protocol::{Conversation, HookHeard, HookOrigin, Typing};
use krad_runtime::{cli_name, BusReply};
use std::collections::BTreeSet;

use super::ask::HeldAskReport;
use super::git_actions::GitFollowUp;
use super::shell_site::ShellSite;
use super::workers::WorkerLaunch;
use super::EnvironmentShell;

const FRESH_CONVERSATION: &str = "Fresh conversation";
const FRESH_TOAST: &str = "could not resume its conversation and started a fresh one";
pub(super) const NO_GROUP: &str = "Run spawn from a terminal that belongs to a group";
const UNKNOWN_ACP_AGENT: &str = "is not an ACP agent; add it in Settings, ACP agents";
pub(super) const NOT_A_REPO: &str =
    "The caller's group is not in a git repository, so it cannot branch a worktree";
const CLOUD_WORKTREE: &str =
    "Workers of a cloud session run in its worktree on the cloud runner; start them without --worktree";
pub(super) const BAD_WORKTREE_NAME: &str =
    "Use letters, digits, '-', '_' or '.' for the worktree name";

pub(super) struct WorkerOrder {
    pub(super) preset: AgentPreset,
    pub(super) worktree: Option<String>,
    pub(super) brief: String,
    pub(super) lead: Option<WorkerOf>,
    pub(super) mode: Option<PermissionMode>,
    pub(super) cloud: Option<CloudPlacement>,
}

pub(super) struct WorkerOf {
    pub(super) session: SessionId,
    pub(super) task: String,
}

impl EnvironmentShell {
    pub(super) fn agent_launch_line(
        &self,
        (env, id): (&EnvironmentName, TerminalId),
        preset: AgentPreset,
        site: ShellSite,
        prompt: &str,
        model: Option<&ModelId>,
    ) -> Option<(String, Option<String>)> {
        let mut line = self.settings.launch_line(preset)?;
        let quoting = site.quoting();
        if let (Some(model), Some(ModelDelivery::Flag(flag))) = (model, preset.model_delivery()) {
            let model = quote_for_shell(model.as_str(), quoting);
            line = format!("{line} {flag} {model}");
        }
        let cli = cli_name(site.is_wsl());
        let remote = self.remote_bus(env).filter(|_| site == ShellSite::Ssh);
        let cloud = self
            .cloud_hook_settings(env, id)
            .filter(|_| site == ShellSite::Cloud);
        let reaches = site.reaches_bus() || remote.is_some() || cloud.is_some();
        if let Some(bus) = self.runtime.handle.bus().filter(|_| reaches) {
            let settings = match (remote, cloud) {
                (Some(remote), _) => Some(remote.claude_settings.to_string_lossy().into_owned()),
                (None, Some(cloud)) => Some(cloud),
                (None, None) => bus
                    .claude_settings
                    .as_ref()
                    .filter(|_| site.reads_local_files())
                    .map(|path| path.to_string_lossy().into_owned()),
            };
            if let Some(args) = hook_launch_args(preset, cli, quoting, settings.as_deref()) {
                line = format!("{line} {args}");
            }
            if self.agent_tools_offered() {
                if let Some(args) = code_tools_launch_args(preset, cli, quoting) {
                    line = format!("{line} {args}");
                }
            }
            if let Some(flag) = preset.system_prompt_flag() {
                let hint = self
                    .role_of(env, id)
                    .hint()
                    .replace(krad_agents::CLI_PROGRAM, cli);
                line = format!("{line} {flag} {}", quote_for_shell(&hint, quoting));
            }
        }
        Some(launch_with_prompt(&line, preset, prompt, quoting))
    }

    pub(super) fn queue_prompt(&self, env: &EnvironmentName, id: TerminalId, text: String) {
        self.type_typing(env, id, Typing::Prompt { text });
    }

    pub(super) fn hook_heard(&mut self, heard: HookHeard) {
        let HookHeard {
            agent,
            harness,
            event,
            tool,
            mode,
            state,
            activity,
            conversation,
            origin,
        } = heard;
        let (env, id) = (agent.environment, agent.terminal);
        self.mark_terminal_hooked(&env, id);
        if let Some(session) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.sessions.get_mut(&id))
        {
            if state.is_some() {
                session.activity = activity;
            }
            if conversation == Conversation::StartedFresh {
                session.activity = Some(FRESH_CONVERSATION.to_owned());
            }
        }
        if conversation == Conversation::StartedFresh {
            self.push_toast(
                ToastKind::Info,
                format!("terminal #{} {FRESH_TOAST}", id.as_u64()),
            );
        }
        let held = HeldAskReport {
            preset: harness,
            event: &event,
            tool: tool.as_deref(),
            mode,
        };
        self.settle_held_asks(&env, id, &held);
        let Some(state) = state else {
            return;
        };
        let delivered = match origin {
            HookOrigin::Replayed { delivered } => delivered,
            HookOrigin::Live => BTreeSet::new(),
        };
        self.with_webhooks_delivered(delivered, |shell| {
            shell.set_agent_state(&env, id, state);
        });
    }

    pub(super) fn spawn_worker(
        &mut self,
        from: &TerminalHandle,
        preset: AgentPreset,
        worktree: Option<String>,
        text: &str,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        let env = EnvironmentName::new(from.environment.clone());
        let caller = TerminalId(from.terminal);
        let Some(group) = self
            .envs
            .get(&env)
            .and_then(|live| live.data.group_of(caller))
            .cloned()
        else {
            reply
                .send(Response::error(ErrorCode::Unsupported, NO_GROUP))
                .unwrap_or_default();
            return;
        };
        let in_wsl = krad_domain::parse_wsl_path(&group.root).is_some();
        let brief = worker_brief(from.terminal, cli_name(in_wsl), text);
        let order = WorkerOrder {
            preset,
            worktree,
            brief,
            lead: None,
            mode: self.chat_permission_mode(&env, caller),
            cloud: self.cloud_placement_of(&env, caller),
        };
        self.start_worker(&env, &group, order, reply, cx);
    }

    pub(super) fn start_worker(
        &mut self,
        env: &EnvironmentName,
        group: &TerminalGroup,
        order: WorkerOrder,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        let env = env.clone();
        let WorkerOrder {
            preset,
            worktree,
            brief,
            lead,
            mode,
            cloud,
        } = order;
        if let Some(id) = preset
            .acp_agent()
            .filter(|id| self.settings.acp.agent(*id).is_none())
        {
            reply
                .send(Response::error(
                    ErrorCode::NotFound,
                    format!("{id} {UNKNOWN_ACP_AGENT}"),
                ))
                .unwrap_or_default();
            return;
        }
        let lead_task = lead.as_ref().map(|lead| lead.task.clone());
        let spawn = WorkerSpawn {
            lead: lead.map(|lead| lead.session),
            task: lead_task.unwrap_or_default(),
        };
        if cloud.is_some() && worktree.is_some() {
            reply
                .send(Response::error(ErrorCode::Unsupported, CLOUD_WORKTREE))
                .unwrap_or_default();
            return;
        }
        let Some(name) = worktree else {
            let launch = WorkerLaunch {
                group: Some(group.id),
                preset,
                brief,
                spawn,
                mode,
                cloud,
            };
            self.launch_worker(&env, launch, reply, cx);
            return;
        };
        if self.git_at(&env, &group.root).is_none() {
            reply
                .send(Response::error(ErrorCode::Unsupported, NOT_A_REPO))
                .unwrap_or_default();
            return;
        }
        let Some(name) = sanitize_worktree_name(&name) else {
            reply
                .send(Response::error(ErrorCode::BadRequest, BAD_WORKTREE_NAME))
                .unwrap_or_default();
            return;
        };
        let launch = Box::new(WorkerLaunch {
            group: None,
            preset,
            brief,
            spawn,
            mode,
            cloud: None,
        });
        let follow = GitFollowUp::LaunchWorker { launch, reply };
        self.create_worktree_then(&env, group.id, &name, follow, cx);
    }
}
