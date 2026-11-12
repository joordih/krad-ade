use gpui_kit::Context;
use krad_chat::{
    BusVars, ChatBackend, ChatEvent, EngineConfig, LiveConfig, OpencodeServer, Runner,
};
use krad_domain::{AgentPreset, AgentState, AttentionKind, ChatEntry, EnvironmentName, TerminalId};
use krad_host::HostLink;
use krad_store::gateway_sessions_dir;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::super::super::agent_bus::{cli_name, BUS_HINT};
use super::super::super::agent_hooks::LEAD_HINT;
use super::super::super::chat::{LiveProcess, StartError};
use super::super::super::cloud::NodeSite;
use super::super::super::gateway::GatewayReach;
use super::super::super::kraddie_memory::joined_prompt;
use super::super::super::opencode::OpencodeLink;
use super::super::super::terminal_card::home_dir;
use super::super::super::EnvironmentShell;
use super::super::{Backend, SurfaceChat};
use super::{add_notice, structured_chat, structured_mut, Batch};

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn flush_structured(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let has_work = structured_chat(self, env, id).is_some_and(|chat| match &chat.backend {
            Backend::Structured(structured) => structured.has_work(&chat.outbox),
            Backend::Transcript(_) => false,
        });
        if !has_work || !self.start_or_report(env, id, cx) {
            return;
        }
        let Some(SurfaceChat {
            backend: Backend::Structured(structured),
            outbox,
            messages,
            problem,
            ..
        }) = structured_chat(self, env, id)
        else {
            return;
        };
        let batch = structured.take_batch(outbox, Instant::now());
        if let Some(prompt) = &batch.message {
            outbox.delivered();
            structured.fold_sent(messages, prompt);
            *problem = None;
        }
        for notice in std::mem::take(&mut structured.notices) {
            add_notice(problem, &notice);
        }
        if batch.message.is_some() && structured.harness.checkpoints_files() {
            return self.hand_over_after_checkpoint(env, id, batch, cx);
        }
        self.hand_over(env, id, batch);
    }

    pub(super) fn hand_over_again(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        batch: Batch,
        failure: ChatEvent,
        cx: &mut Context<Self>,
    ) {
        if self.start_or_report(env, id, cx) {
            self.hand_over(env, id, batch);
            return;
        }
        if let Some(SurfaceChat {
            backend: Backend::Structured(structured),
            messages,
            ..
        }) = structured_chat(self, env, id)
        {
            structured.closes_turn(true);
            for outcome in structured.asks.cancel_all() {
                structured.fold_kept(messages, &ChatEntry::Asked(outcome));
            }
            structured.fold_kept(messages, &ChatEntry::Event(failure));
        }
        self.set_agent_state(env, id, AgentState::Idle);
        self.raise_attention(env, id, AttentionKind::Error);
    }

    pub(super) fn hand_over(&mut self, env: &EnvironmentName, id: TerminalId, batch: Batch) {
        let Some(structured) = structured_mut(self, env, id) else {
            return;
        };
        let Some(live) = structured.live() else {
            return;
        };
        for command in &batch.commands {
            live.run(command);
        }
        if let Some(prompt) = &batch.message {
            live.send(prompt.clone());
        }
        let starts_turn = batch.message.is_some();
        structured.handed_over(batch);
        if starts_turn {
            self.set_agent_state(env, id, AgentState::Working);
        }
    }

    pub(super) fn start_or_report(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Err(error) = self.ensure_live(env, id, cx) else {
            return true;
        };
        if matches!(error, StartError::Waiting) {
            self.check_cloud_processes(cx);
            return false;
        }
        if let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.problem = Some(error.to_string());
        }
        self.fail_worker_launch(env, id, &error.to_string(), cx);
        false
    }

    fn bus_hint(&self, in_wsl: bool) -> Option<String> {
        self.agent_bus
            .is_some()
            .then(|| BUS_HINT.replace(krad_agents::CLI_PROGRAM, cli_name(in_wsl)))
    }

    fn lead_hint(&self, in_wsl: bool) -> Option<String> {
        self.agent_bus
            .is_some()
            .then(|| LEAD_HINT.replace(krad_agents::CLI_PROGRAM, cli_name(in_wsl)))
    }

    fn orchestrates_in(&self, env: &EnvironmentName, id: TerminalId) -> bool {
        self.envs
            .get(env)
            .is_some_and(|live| live.data.session_led_by(id).is_some())
    }

    fn ensure_live(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> Result<(), StartError> {
        let Some(harness) = structured_mut(self, env, id)
            .filter(|structured| structured.live().is_none())
            .map(|structured| structured.harness)
        else {
            return Ok(());
        };
        match self.node_site(env, id) {
            NodeSite::Waiting(runner) => {
                self.note_cloud_wait(env, id, runner, cx);
                return Err(StartError::Waiting);
            }
            NodeSite::Env if self.awaits_remote_bus(env) => return Err(StartError::Waiting),
            NodeSite::Cloud(_) if self.awaits_runner_bus(env, id) => {
                return Err(StartError::Waiting)
            }
            NodeSite::Cloud(_) | NodeSite::Env => {}
        }
        let backend = self.chat_backend(env, id, harness, cx)?;
        let owner = env.clone();
        let apply = move |shell: &mut Self, event, cx: &mut Context<Self>| {
            shell.note_prompted(&owner, id, &event);
            shell.apply_structured_event(&owner, id, event, cx);
        };
        let Some(structured) = structured_mut(self, env, id) else {
            return Ok(());
        };
        structured.live = Some(LiveProcess::spawn(backend, &structured.session, cx, apply)?);
        Ok(())
    }

    pub(in crate::environment_shell) fn chat_backend(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        harness: AgentPreset,
        cx: &mut Context<Self>,
    ) -> Result<ChatBackend, StartError> {
        let site = self.node_site(env, id);
        if let (true, Some(reason)) = (site.is_cloud(), harness.unavailable_in_cloud()) {
            return Err(StartError::Cloud(reason.to_owned()));
        }
        Ok(match harness {
            AgentPreset::ClaudeCode => ChatBackend::Claude(self.live_config(env, id, harness)?),
            AgentPreset::Codex => ChatBackend::Codex(self.live_config(env, id, harness)?),
            AgentPreset::Opencode => {
                let config = self.live_config(env, id, harness)?;
                let server = match &site {
                    NodeSite::Cloud(cloud) => self.cloud_opencode_for(cloud, cx)?,
                    NodeSite::Env | NodeSite::Waiting(_) => {
                        self.opencode_for(&config.runner, &config.cwd, harness, cx)?
                    }
                };
                ChatBackend::Opencode(server, config)
            }
            AgentPreset::Gateway => ChatBackend::Gateway(self.gateway_config(env, id, cx)?),
            AgentPreset::Acp(agent) => ChatBackend::Acp(self.acp_config(env, id, agent, cx)?),
            AgentPreset::Shell => {
                return Err(StartError::NoChat(self.settings.acp.harness_name(harness)))
            }
        })
    }

    fn system_prompt(&self, env: &EnvironmentName, id: TerminalId, in_wsl: bool) -> Option<String> {
        let hint = if self.orchestrates_in(env, id) {
            self.lead_hint(in_wsl)
        } else {
            self.bus_hint(in_wsl)
        };
        joined_prompt(hint, self.memory_preamble(env, id))
    }

    fn live_config(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        harness: AgentPreset,
    ) -> Result<LiveConfig, StartError> {
        let (runner, cwd) = match self.node_site(env, id) {
            NodeSite::Env => {
                let root = self
                    .envs
                    .get(env)
                    .and_then(|live| live.data.group_of(id))
                    .map(|group| group.root.clone());
                self.harness_runner(&self.env_link(env), harness, root.as_deref())?
            }
            NodeSite::Cloud(site) => {
                let program = self.launch_program(harness)?;
                self.cloud_chat_runner(env, id, &site, harness, program)?
            }
            NodeSite::Waiting(_) => return Err(StartError::Waiting),
        };
        let vars = self.bus_vars(env, id);
        let off_bus = if runner.in_cloud() {
            vars.is_empty()
        } else {
            runner.on_ssh() && self.remote_bus(env).is_none()
        };
        let (system_prompt, mut bus_vars) = if off_bus {
            (self.memory_preamble(env, id), BusVars(Vec::new()))
        } else {
            (self.system_prompt(env, id, runner.in_wsl()), BusVars(vars))
        };
        bus_vars.0.extend(
            self.cloud_claude_env(env, id, harness)
                .filter(|_| runner.in_cloud()),
        );
        let (model, notice) = self.offered_choice(env, id, harness);
        let usage_budget = self.claude_usage_budget();
        let structured = structured_mut(self, env, id).ok_or(StartError::Waiting)?;
        structured.notices.extend(notice);
        Ok(LiveConfig {
            runner,
            model,
            cwd,
            system_prompt,
            effort: structured.effort,
            permission_mode: structured.permission_mode,
            bus_vars,
            usage_budget,
        })
    }

    fn opencode_for(
        &mut self,
        runner: &Runner,
        cwd: &Path,
        harness: AgentPreset,
        cx: &mut Context<Self>,
    ) -> Result<Arc<OpencodeServer>, StartError> {
        if runner.in_wsl() {
            return Err(StartError::NotInWsl(
                self.settings.acp.harness_name(harness),
            ));
        }
        if runner.on_ssh() {
            return match self.opencode.hosts.get(&runner.link().host()) {
                Some(OpencodeLink::Ready { server, .. }) => Ok(Arc::clone(server)),
                Some(OpencodeLink::Failed(error)) => Err(StartError::Opencode(error.clone())),
                Some(OpencodeLink::Absent | OpencodeLink::Starting) => Err(StartError::Waiting),
                None => {
                    self.ensure_host_opencode(runner.clone(), cwd.to_path_buf(), cx);
                    Err(StartError::Waiting)
                }
            };
        }
        match &self.opencode.link {
            OpencodeLink::Ready { server, .. } => Ok(Arc::clone(server)),
            OpencodeLink::Failed(error) => Err(StartError::Opencode(error.clone())),
            OpencodeLink::Absent | OpencodeLink::Starting => {
                self.ensure_opencode(cx);
                Err(StartError::Waiting)
            }
        }
    }

    fn gateway_config(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> Result<EngineConfig, StartError> {
        if !self.env_link(env).is_ready() {
            return Err(StartError::Waiting);
        }
        let access = match self.gateway_reach() {
            GatewayReach::Ready(access) => access,
            GatewayReach::Waiting => return Err(StartError::Waiting),
            GatewayReach::Unavailable(error) => return Err(StartError::Gateway(error)),
        };
        let sessions = gateway_sessions_dir().ok_or(StartError::NoConfigFolder)?;
        let cwd = self.chat_folder(env, id);
        let launch = self
            .gateway_mcp(env, id, &cwd, cx)
            .ok_or(StartError::Waiting)?;
        let host = self.env_link(env);
        let (system_prompt, bus_vars, home) = match &host {
            HostLink::Local => (
                self.system_prompt(env, id, false),
                BusVars(self.bus_vars(env, id)),
                home_dir().map(PathBuf::from),
            ),
            HostLink::Remote(ssh) => {
                let home = ssh.home().map(Path::to_path_buf);
                if self.remote_bus(env).is_some() {
                    (
                        self.system_prompt(env, id, false),
                        BusVars(self.bus_vars(env, id)),
                        home,
                    )
                } else {
                    (self.memory_preamble(env, id), BusVars(Vec::new()), home)
                }
            }
        };
        let chosen = structured_mut(self, env, id).and_then(|chat| chat.chosen_model.clone());
        let model = self
            .gateway_model(chosen.as_ref())
            .ok_or_else(|| StartError::Gateway(Self::no_gateway_model_text().to_owned()))?;
        let served = self.gateway_served().to_vec();
        let context_windows = self.settings.gateway.context_windows.clone();
        let structured = structured_mut(self, env, id).ok_or(StartError::Waiting)?;
        structured.notices.extend(launch.problems);
        Ok(EngineConfig {
            access,
            sessions,
            home,
            cwd,
            model: Some(model),
            models: served,
            context_windows,
            system_prompt,
            effort: structured.effort,
            permission_mode: structured.permission_mode,
            bus_vars,
            mcp: launch.starts,
            host,
            skills: self.settings.skills.clone(),
        })
    }

    pub(in crate::environment_shell) fn flush_chats_of(
        &mut self,
        harness: AgentPreset,
        cx: &mut Context<Self>,
    ) {
        let waiting: Vec<(EnvironmentName, TerminalId)> = self
            .envs
            .iter()
            .flat_map(|(env, live)| {
                live.chats
                    .iter()
                    .filter(|(_, chat)| {
                        chat.structured()
                            .is_some_and(|structured| structured.harness == harness)
                    })
                    .map(move |(id, _)| (env.clone(), *id))
            })
            .collect();
        for (env, id) in waiting {
            self.flush_structured(&env, id, cx);
        }
    }
}
