use krad_chat::Runner;
use krad_domain::AgentPreset;
use krad_protocol::AgentRef;

use super::super::backend::Unready;
use super::super::brief::{ChatBrief, ChatRole};
use super::super::cloud::CloudChat;
use super::{caller, problem, LaunchFacts, LiveParts, NO_PROGRAM_TEXT};
use crate::hosts::remote_detached_caller_vars;

const TOKEN_VAR: &str = "CLAUDE_CODE_OAUTH_TOKEN";

impl LaunchFacts<'_> {
    pub(in crate::chats) fn cloud_program(&self, harness: AgentPreset) -> Result<String, Unready> {
        let Some(agent) = harness.acp_agent() else {
            return self.program(harness);
        };
        self.settings
            .acp
            .agent(agent)
            .map(|agent| agent.program)
            .ok_or_else(|| problem(self.settings.acp.harness_name(harness), NO_PROGRAM_TEXT))
    }

    pub(in crate::chats) fn cloud_runner(cloud: &CloudChat, program: String) -> Runner {
        match &cloud.run {
            Some(run) => Runner::Cloud(run.clone()),
            None => Runner::Ssh {
                link: cloud.site.link.clone(),
                program,
            },
        }
    }

    pub(in crate::chats) fn cloud_bus_vars(
        &self,
        chat: &AgentRef,
        harness: AgentPreset,
        cloud: &CloudChat,
    ) -> Vec<(String, String)> {
        let mut vars = self
            .runners
            .buses
            .ready(cloud.site.placement.runner)
            .map_or_else(Vec::new, |bus| {
                remote_detached_caller_vars(bus, &caller(chat))
            });
        let token = cloud
            .site
            .claude_token
            .as_ref()
            .filter(|_| harness == AgentPreset::ClaudeCode && cloud.run.is_some());
        if let Some(token) = token {
            vars.push((TOKEN_VAR.to_owned(), token.expose().to_owned()));
        }
        vars
    }

    pub(in crate::chats) fn cloud_prompt(
        &self,
        chat: &AgentRef,
        brief: &ChatBrief,
        cloud: &CloudChat,
    ) -> Option<String> {
        let memory = self.memory(chat);
        let on_bus = self
            .runners
            .buses
            .ready(cloud.site.placement.runner)
            .is_some();
        match brief.role {
            ChatRole::Digest => None,
            ChatRole::Agent if on_bus => {
                self.system_prompt(self.role(chat), false, memory.as_deref())
            }
            ChatRole::Agent => memory,
        }
    }

    pub(super) fn cloud_live(
        &self,
        chat: &AgentRef,
        harness: AgentPreset,
        brief: &ChatBrief,
        cloud: &CloudChat,
    ) -> Result<LiveParts, Unready> {
        let runner = Self::cloud_runner(cloud, self.program(harness)?);
        let cwd = cloud.site.placement.worktree.clone();
        let vars = match brief.role {
            ChatRole::Digest => Vec::new(),
            ChatRole::Agent => self.cloud_bus_vars(chat, harness, cloud),
        };
        Ok((runner, cwd, self.cloud_prompt(chat, brief, cloud), vars))
    }
}
