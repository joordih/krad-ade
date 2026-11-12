use krad_chat::{copy_claude_conversation, new_session_id, ConversationMove, LiveSession};
use krad_domain::{
    fork_seed_prompt, AgentPreset, EnvironmentName, HistoryMessage, Prompt, ResumeId, SessionId,
};
use krad_host::HostLink;
use std::path::{Path, PathBuf};

use super::super::chat::workplace_home;
use super::super::kraddie_memory::history_of;
use super::super::sessions_view::session_terminals;
use super::super::EnvironmentShell;
use super::naming::cloud_chat_harness;

const CARRIED_MESSAGES: usize = 40;

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct Transcript {
    link: HostLink,
    root: Option<PathBuf>,
    resume: ResumeId,
}

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct CarriedChat {
    harness: AgentPreset,
    transcript: Option<Transcript>,
    seed: Option<String>,
}

pub(in crate::environment_shell) struct CarriedStart {
    pub(in crate::environment_shell) harness: AgentPreset,
    pub(in crate::environment_shell) session: LiveSession,
    pub(in crate::environment_shell) prompts: Vec<Prompt>,
}

fn last_turns(messages: &[HistoryMessage]) -> &[HistoryMessage] {
    &messages[messages.len().saturating_sub(CARRIED_MESSAGES)..]
}

fn fresh_start(harness: AgentPreset, seed: Option<String>) -> CarriedStart {
    CarriedStart {
        harness,
        session: LiveSession::Start(new_session_id()),
        prompts: seed.map_or_else(Vec::new, |seed| vec![Prompt::new(&seed, Vec::new())]),
    }
}

fn forked(
    transcript: &Transcript,
    runner: &HostLink,
    runner_home: &Path,
    worktree: &Path,
) -> Option<LiveSession> {
    let home = workplace_home(&transcript.link, transcript.root.as_deref())
        .inspect_err(|error| tracing::debug!(%error, "the chat's home folder is unknown"))
        .ok()
        .flatten()?;
    let route = ConversationMove {
        from: &transcript.link,
        from_home: &home,
        from_cwd: transcript.root.as_deref(),
        to: runner,
        to_home: runner_home,
        to_cwd: worktree,
    };
    match copy_claude_conversation(&route, &transcript.resume) {
        Ok(_) => Some(LiveSession::Fork(transcript.resume.clone())),
        Err(error) => {
            tracing::info!(%error, "the chat starts fresh on the cloud runner");
            None
        }
    }
}

#[tracing::instrument(skip_all, fields(chats = carried.len()))]
pub(in crate::environment_shell) fn carry_chats(
    runner: &HostLink,
    worktree: &Path,
    carried: Vec<CarriedChat>,
) -> Vec<CarriedStart> {
    let runner_home = match runner {
        HostLink::Remote(ssh) => ssh.home().map(Path::to_path_buf),
        HostLink::Local => None,
    };
    carried
        .into_iter()
        .map(|chat| {
            let session = chat
                .transcript
                .as_ref()
                .zip(runner_home.as_deref())
                .and_then(|(transcript, home)| forked(transcript, runner, home, worktree));
            match session {
                Some(session) => CarriedStart {
                    harness: chat.harness,
                    session,
                    prompts: Vec::new(),
                },
                None => fresh_start(chat.harness, chat.seed),
            }
        })
        .collect()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn carried_chats(
        &self,
        env: &EnvironmentName,
        session: SessionId,
    ) -> Vec<CarriedChat> {
        let Some(live) = self.envs.get(env) else {
            return Vec::new();
        };
        let Some(found) = live.data.session(session) else {
            return Vec::new();
        };
        let root = found
            .group
            .and_then(|group| live.data.group(group))
            .map(|group| group.root.clone());
        let candidates = AgentPreset::chat_harnesses(&self.settings.acp.all_agents());
        session_terminals(found)
            .filter_map(|id| {
                let node = live.data.nodes.iter().find(|node| node.id == id)?;
                let harness = node.harness.filter(|preset| preset.has_structured_chat())?;
                let chat = live.chats.get(&id)?;
                let messages = history_of(&chat.messages);
                let seed = (!messages.is_empty()).then(|| {
                    fork_seed_prompt(
                        &node.title,
                        &self.settings.acp.harness_name(harness),
                        last_turns(&messages),
                    )
                });
                let transcript = node
                    .resume
                    .clone()
                    .filter(|_| harness == AgentPreset::ClaudeCode)
                    .map(|resume| Transcript {
                        link: self.env_link(env),
                        root: root.clone(),
                        resume,
                    });
                let harness = cloud_chat_harness(harness, &candidates);
                Some(CarriedChat {
                    transcript: transcript.filter(|_| harness == AgentPreset::ClaudeCode),
                    harness,
                    seed,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::HistorySpeaker;

    #[test]
    fn a_long_conversation_carries_only_its_latest_turns() {
        let messages: Vec<HistoryMessage> = (0..CARRIED_MESSAGES + 5)
            .map(|index| HistoryMessage {
                speaker: HistorySpeaker::User,
                text: index.to_string(),
            })
            .collect();
        let kept = last_turns(&messages);
        assert_eq!(kept.len(), CARRIED_MESSAGES);
        assert_eq!(kept.first().map(|message| message.text.as_str()), Some("5"));
        assert_eq!(last_turns(&messages[..2]).len(), 2);
    }

    #[test]
    fn a_chat_that_cannot_be_copied_starts_with_its_summary() {
        let start = fresh_start(AgentPreset::Codex, Some("earlier".to_owned()));
        assert!(matches!(start.session, LiveSession::Start(_)));
        assert_eq!(start.prompts.len(), 1);
        assert_eq!(fresh_start(AgentPreset::Codex, None).prompts.len(), 0);
    }
}
