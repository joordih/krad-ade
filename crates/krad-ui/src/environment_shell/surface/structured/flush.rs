use gpui_kit::Context;
use krad_chat::{Approvals, BusVars, ChatEvent, LiveConfig};
use krad_domain::{AgentPreset, AgentState, ChatEntry, EnvironmentName, TerminalId};
use std::time::Instant;

use super::super::super::agent_bus::{cli_name, BUS_HINT};
use super::super::super::agent_hooks::LEAD_HINT;
use super::super::super::chat::{fold_entry, LiveProcess, StartError};
use super::super::super::opencode::OpencodeLink;
use super::super::super::EnvironmentShell;
use super::super::{Backend, SurfaceChat};
use super::{structured_chat, structured_mut, Batch};

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
        if let Some(text) = &batch.message {
            outbox.delivered();
            fold_entry(
                messages,
                &ChatEntry::User {
                    text: text.clone(),
                    preset: structured.harness,
                    model: None,
                },
            );
            *problem = None;
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
                fold_entry(messages, &ChatEntry::Asked(outcome));
            }
            fold_entry(messages, &ChatEntry::Event(failure));
        }
        self.set_agent_state(env, id, AgentState::Idle);
    }

    fn hand_over(&mut self, env: &EnvironmentName, id: TerminalId, batch: Batch) {
        let Some(structured) = structured_mut(self, env, id) else {
            return;
        };
        let Some(live) = structured.live() else {
            return;
        };
        for command in &batch.commands {
            live.run(command);
        }
        if let Some(text) = &batch.message {
            live.send(text);
        }
        let starts_turn = batch.message.is_some();
        structured.handed_over(batch);
        if starts_turn {
            self.set_agent_state(env, id, AgentState::Working);
        }
    }

    fn start_or_report(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Err(error) = self.ensure_live(env, id, cx) else {
            return true;
        };
        if matches!(error, StartError::Waiting) {
            return false;
        }
        if let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.problem = Some(error.to_string());
        }
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
        if structured_mut(self, env, id).is_some_and(|structured| structured.live().is_some()) {
            return Ok(());
        }
        let root = self
            .envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone());
        let harness =
            structured_mut(self, env, id).map_or(AgentPreset::ClaudeCode, |chat| chat.harness);
        let (runner, cwd) = self.harness_runner(harness, root.as_deref())?;
        let opencode = match (harness, &self.opencode.link) {
            (AgentPreset::Opencode, _) if runner.in_wsl() => {
                return Err(StartError::NotInWsl(harness));
            }
            (AgentPreset::Opencode, OpencodeLink::Ready { server, .. }) => Some(server.clone()),
            (AgentPreset::Opencode, OpencodeLink::Failed(error)) => {
                return Err(StartError::Opencode(error.clone()));
            }
            (AgentPreset::Opencode, OpencodeLink::Absent | OpencodeLink::Starting) => {
                self.ensure_opencode(cx);
                return Err(StartError::Waiting);
            }
            _ => None,
        };
        let system_prompt = if self.orchestrates_in(env, id) {
            self.lead_hint(runner.in_wsl())
        } else {
            self.bus_hint(runner.in_wsl())
        };
        let bus_vars = BusVars(self.bus_vars(env, id));
        let Some(structured) = structured_mut(self, env, id) else {
            return Ok(());
        };
        let config = LiveConfig {
            runner,
            model: structured.chosen_model.clone(),
            cwd,
            extra_dirs: Vec::new(),
            orchestration: None,
            system_prompt,
            effort: structured.effort,
            permission_mode: structured.permission_mode,
            bus_vars,
            approvals: Approvals::Asked,
        };
        let owner = env.clone();
        let apply = move |shell: &mut Self, event, cx: &mut Context<Self>| {
            shell.apply_structured_event(&owner, id, event, cx);
        };
        let live = match (opencode, harness) {
            (Some(server), _) => {
                LiveProcess::spawn_opencode(server, config, &structured.session, cx, apply)
            }
            (None, AgentPreset::Codex) => {
                LiveProcess::spawn_codex(config, &structured.session, cx, apply)
            }
            (None, _) => LiveProcess::spawn(config, &structured.session, cx, apply)?,
        };
        structured.live = Some(live);
        Ok(())
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
