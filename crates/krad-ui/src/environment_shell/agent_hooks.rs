use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::{AppContext as _, Context};
use krad_agents::{AgentStatus, Response, TerminalHandle};
use krad_domain::{
    code_tools_launch_args, hook_launch_args, launch_with_prompt, quote_for_shell, worker_brief,
    AgentPreset, AgentState, EnvironmentName, ModelDelivery, ModelId, PermissionMode, ResumeId,
    SessionId, TerminalGroup, TerminalId, WorkerSpawn, WorktreeOrigin,
};
use krad_repo::{add_worktree, sanitize_worktree_name};
use std::time::{Duration, Instant};

use super::agent_bus::{cli_name, BUS_HINT};
use super::ask::HeldAskReport;
use super::shell_site::ShellSite;
use super::surface::AccountUsage;
use super::workers::WorkerLaunch;
use super::EnvironmentShell;

const PROMPT_DELAY: Duration = Duration::from_secs(4);
const FRESH_CONVERSATION: &str = "Fresh conversation";
const FRESH_TOAST: &str = "could not resume its conversation and started a fresh one";
pub(super) const NO_GROUP: &str = "Run spawn from a terminal that belongs to a group";
const UNKNOWN_ACP_AGENT: &str = "is not an ACP agent; add it in Settings, ACP agents";
const NOT_A_REPO: &str =
    "The caller's group is not in a git repository, so it cannot branch a worktree";
const BAD_WORKTREE_NAME: &str = "Use letters, digits, '-', '_' or '.' for the worktree name";

pub(super) struct HookReport<'a> {
    pub(super) harness: AgentPreset,
    pub(super) event: &'a str,
    pub(super) tool: Option<&'a str>,
    pub(super) detail: Option<String>,
    pub(super) resume: Option<&'a str>,
    pub(super) mode: Option<PermissionMode>,
}

pub(super) struct PendingPrompt {
    pub(super) target: TerminalHandle,
    pub(super) text: String,
    pub(super) due: Instant,
}

pub(super) struct WorkerOrder {
    pub(super) preset: AgentPreset,
    pub(super) worktree: Option<String>,
    pub(super) brief: String,
    pub(super) lead: Option<WorkerOf>,
    pub(super) mode: Option<PermissionMode>,
}

pub(super) struct WorkerOf {
    pub(super) session: SessionId,
    pub(super) task: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AgentRole {
    Member,
    Lead,
}

impl AgentRole {
    const fn hint(self) -> &'static str {
        match self {
            Self::Member => BUS_HINT,
            Self::Lead => LEAD_HINT,
        }
    }
}

pub(super) const LEAD_HINT: &str = "You lead this Krad ADE session: you split the work and start and direct the other coding agents of its group. Run 'krad-ade-agents help lead' first to learn how, and follow it.";

impl EnvironmentShell {
    pub(super) fn agent_launch_line(
        &self,
        env: &EnvironmentName,
        preset: AgentPreset,
        site: ShellSite,
        prompt: &str,
        model: Option<&ModelId>,
        role: AgentRole,
    ) -> Option<(String, Option<String>)> {
        let mut line = self.settings.launch_line(preset)?;
        if let (Some(model), Some(ModelDelivery::Flag(flag))) = (model, preset.model_delivery()) {
            line = format!("{line} {flag} {model}");
        }
        let quoting = site.quoting();
        let cli = cli_name(site.is_wsl());
        let remote = self.remote_bus(env).filter(|_| site == ShellSite::Ssh);
        let reaches = site.reaches_bus() || remote.is_some();
        if self.agent_bus.is_some() && reaches {
            let settings = match remote {
                Some(remote) => Some(remote.claude_settings.to_string_lossy().into_owned()),
                None => self
                    .agent_bus
                    .as_ref()
                    .and_then(|bus| bus.claude_settings.as_ref())
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
                let hint = role.hint().replace(krad_agents::CLI_PROGRAM, cli);
                line = format!("{line} {flag} {}", quote_for_shell(&hint, quoting));
            }
        }
        Some(launch_with_prompt(&line, preset, prompt, quoting))
    }

    pub(super) fn queue_prompt(&mut self, env: &EnvironmentName, id: TerminalId, text: String) {
        if let Some(bus) = &mut self.agent_bus {
            bus.pending_prompts.push(PendingPrompt {
                target: TerminalHandle {
                    environment: env.as_str().to_owned(),
                    terminal: id.as_u64(),
                },
                text,
                due: Instant::now() + PROMPT_DELAY,
            });
        }
    }

    pub(super) fn apply_status(&mut self, from: &TerminalHandle, status: AgentStatus) -> Response {
        let env = EnvironmentName::new(from.environment.clone());
        self.account_usage = merged_usage(self.account_usage, &status);
        if let Some(live) = self.envs.get_mut(&env) {
            live.statuses.insert(TerminalId(from.terminal), status);
        }
        Response::Noted
    }

    pub(super) fn apply_hook(&mut self, from: &TerminalHandle, report: HookReport<'_>) -> Response {
        let env = EnvironmentName::new(from.environment.clone());
        let id = TerminalId(from.terminal);
        let preset = report.harness;
        let signal = preset.hook_signal(report.event, report.tool);
        if let Some(session) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.sessions.get_mut(&id))
        {
            session.hooked = true;
            if signal.is_some() {
                session.activity = report.detail;
            }
        }
        let held = HeldAskReport {
            preset,
            event: report.event,
            tool: report.tool,
            mode: report.mode,
        };
        self.settle_held_asks(&env, id, &held);
        let resume = report.resume.and_then(|raw| raw.parse::<ResumeId>().ok());
        let fresh = preset.started_fresh(report.event, report.tool);
        self.note_resume_id(&env, id, preset, resume, fresh);
        if let Some(signal) = signal {
            self.set_agent_state(&env, id, AgentState::from(signal));
        }
        Response::Noted
    }

    fn note_resume_id(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        preset: AgentPreset,
        resume: Option<ResumeId>,
        fresh: bool,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let Some(node) = live.data.nodes.iter_mut().find(|node| node.id == id) else {
            return;
        };
        let Some(resume) = resume.filter(|resume| node.resume.as_ref() != Some(resume)) else {
            return;
        };
        let replaced = node.resume.replace(resume).is_some();
        node.harness = Some(preset);
        if fresh && replaced {
            if let Some(session) = live.sessions.get_mut(&id) {
                session.activity = Some(FRESH_CONVERSATION.to_owned());
            }
            self.push_toast(format!("terminal #{} {FRESH_TOAST}", id.as_u64()));
        }
        self.store();
    }

    pub(super) fn spawn_worker(
        &mut self,
        from: &TerminalHandle,
        preset: AgentPreset,
        worktree: Option<String>,
        text: &str,
        reply: ReplySender<Response>,
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
                .send(Response::Error(NO_GROUP.to_owned()))
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
        };
        self.start_worker(&env, &group, order, reply, cx);
    }

    pub(super) fn start_worker(
        &mut self,
        env: &EnvironmentName,
        group: &TerminalGroup,
        order: WorkerOrder,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let env = env.clone();
        let WorkerOrder {
            preset,
            worktree,
            brief,
            lead,
            mode,
        } = order;
        if let Some(id) = preset
            .acp_agent()
            .filter(|id| self.settings.acp.agent(*id).is_none())
        {
            reply
                .send(Response::Error(format!("{id} {UNKNOWN_ACP_AGENT}")))
                .unwrap_or_default();
            return;
        }
        let lead_task = lead.as_ref().map(|lead| lead.task.clone());
        let spawn = WorkerSpawn {
            lead: lead.map(|lead| lead.session),
            task: lead_task.unwrap_or_default(),
        };
        let Some(name) = worktree else {
            let launch = WorkerLaunch {
                group: Some(group.id),
                preset,
                brief,
                spawn,
                mode,
            };
            self.launch_worker(&env, launch, reply, cx);
            return;
        };
        let Some(repo_root) = self
            .git_at(&env, &group.root)
            .map(|git| git.repo_root.clone())
        else {
            reply
                .send(Response::Error(NOT_A_REPO.to_owned()))
                .unwrap_or_default();
            return;
        };
        let Some(name) = sanitize_worktree_name(&name) else {
            reply
                .send(Response::Error(BAD_WORKTREE_NAME.to_owned()))
                .unwrap_or_default();
            return;
        };
        let link = self.env_link(&env);
        let create = cx.background_spawn({
            let repo_root = repo_root.clone();
            async move { add_worktree(&link, &repo_root, &name) }
        });
        cx.spawn(async move |this, cx| {
            let created = create.await;
            this.update(cx, |shell, cx| match created {
                Ok(worktree) => {
                    let origin = worktree
                        .branch
                        .clone()
                        .map(|branch| WorktreeOrigin { repo_root, branch });
                    let selected = shell.envs.get(&env).and_then(|live| live.selected_group);
                    let group = shell.add_group_to(&env, worktree.path, origin, cx);
                    if let Some(live) = shell.envs.get_mut(&env) {
                        live.selected_group = selected;
                    }
                    let launch = WorkerLaunch {
                        group,
                        preset,
                        brief,
                        spawn,
                        mode,
                    };
                    shell.launch_worker(&env, launch, reply, cx);
                }
                Err(error) => reply
                    .send(Response::Error(error.to_string()))
                    .unwrap_or_default(),
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

fn merged_usage(previous: AccountUsage, status: &AgentStatus) -> AccountUsage {
    AccountUsage {
        five_hour: status.five_hour_used.or(previous.five_hour),
        seven_day: status.seven_day_used.or(previous.seven_day),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_with_one_usage_window_keeps_the_other() {
        let previous = AccountUsage {
            five_hour: Some(10),
            seven_day: Some(40),
        };
        let weekly_only = AgentStatus {
            seven_day_used: Some(45),
            ..AgentStatus::default()
        };
        assert_eq!(
            merged_usage(previous, &weekly_only),
            AccountUsage {
                five_hour: Some(10),
                seven_day: Some(45),
            }
        );
        assert_eq!(merged_usage(previous, &AgentStatus::default()), previous);
    }
}
