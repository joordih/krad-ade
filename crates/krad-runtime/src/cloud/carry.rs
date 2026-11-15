use krad_chat::{copy_claude_conversation, new_session_id, ConversationMove};
use krad_domain::{
    fork_seed_prompt, history_of, AcpSettings, AgentPreset, ChatMessage, EnvironmentData,
    HistoryMessage, Prompt, ResumeId, Session, SessionId, TerminalId,
};
use krad_host::HostLink;
use krad_protocol::{CarriedConversation, CloudChatStart, FirstChat, SentPrompt};
use krad_repo::workplace_home;
use std::path::{Path, PathBuf};

use super::naming::cloud_chat_harness;

const CARRIED_MESSAGES: usize = 40;

#[derive(Clone, Debug)]
pub(crate) struct Transcript {
    link: HostLink,
    root: Option<PathBuf>,
    resume: ResumeId,
}

#[derive(Clone, Debug)]
pub(crate) struct CarriedChat {
    terminal: TerminalId,
    harness: AgentPreset,
    transcript: Option<Transcript>,
    seed: Option<String>,
    prompts: Vec<Prompt>,
}

#[derive(Clone, Debug)]
pub(crate) struct MovedChat {
    pub(crate) terminal: TerminalId,
    pub(crate) start: CloudChatStart,
}

pub(crate) fn with_sent_prompt(carried: &mut [CarriedChat], sent: SentPrompt) {
    let Some(chat) = carried.iter_mut().find(|chat| chat.terminal == sent.chat) else {
        return tracing::warn!(
            chat = sent.chat.as_u64(),
            "the prompt sent with the session belongs to no chat it carries"
        );
    };
    chat.prompts.push(sent.prompt);
}

fn last_turns(messages: &[HistoryMessage]) -> &[HistoryMessage] {
    &messages[messages.len().saturating_sub(CARRIED_MESSAGES)..]
}

pub(crate) fn fresh_start(harness: AgentPreset, seed: Option<String>) -> CloudChatStart {
    CloudChatStart {
        harness,
        conversation: CarriedConversation::Fresh(new_session_id()),
        prompts: seed.map_or_else(Vec::new, |seed| vec![Prompt::new(&seed, Vec::new())]),
    }
}

pub(crate) fn first_start(first: FirstChat, candidates: &[AgentPreset]) -> CloudChatStart {
    CloudChatStart {
        harness: cloud_chat_harness(first.harness, candidates),
        conversation: CarriedConversation::Fresh(new_session_id()),
        prompts: vec![first.prompt],
    }
}

fn forked(
    transcript: &Transcript,
    runner: &HostLink,
    runner_home: &Path,
    worktree: &Path,
) -> Option<CarriedConversation> {
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
        Ok(_) => Some(CarriedConversation::Forked(transcript.resume.clone())),
        Err(error) => {
            tracing::info!(%error, "the chat starts fresh on the cloud runner");
            None
        }
    }
}

#[tracing::instrument(skip_all, fields(chats = carried.len()))]
pub(crate) fn carry_chats(
    runner: &HostLink,
    worktree: &Path,
    carried: Vec<CarriedChat>,
) -> Vec<MovedChat> {
    let runner_home = match runner {
        HostLink::Remote(ssh) => ssh.home().map(Path::to_path_buf),
        HostLink::Local => None,
    };
    carried
        .into_iter()
        .map(|chat| {
            let conversation = chat
                .transcript
                .as_ref()
                .zip(runner_home.as_deref())
                .and_then(|(transcript, home)| forked(transcript, runner, home, worktree));
            let mut start = match conversation {
                Some(conversation) => CloudChatStart {
                    harness: chat.harness,
                    conversation,
                    prompts: Vec::new(),
                },
                None => fresh_start(chat.harness, chat.seed),
            };
            start.prompts.extend(chat.prompts);
            MovedChat {
                terminal: chat.terminal,
                start,
            }
        })
        .collect()
}

pub(crate) struct CarrySource<'a> {
    pub(crate) data: &'a EnvironmentData,
    pub(crate) link: HostLink,
    pub(crate) acp: &'a AcpSettings,
    pub(crate) messages: &'a dyn Fn(TerminalId) -> Option<Vec<ChatMessage>>,
}

fn session_tiles(session: &Session) -> impl Iterator<Item = TerminalId> + '_ {
    session
        .tabs
        .tabs
        .iter()
        .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
}

pub(crate) fn carried_chats(source: &CarrySource<'_>, session: SessionId) -> Vec<CarriedChat> {
    let data = source.data;
    let Some(found) = data.session(session) else {
        return Vec::new();
    };
    let root = found
        .group
        .and_then(|group| data.group(group))
        .map(|group| group.root.clone());
    let candidates = AgentPreset::chat_harnesses(&source.acp.all_agents());
    session_tiles(found)
        .filter_map(|id| {
            let node = data.nodes.iter().find(|node| node.id == id)?;
            let harness = node.harness.filter(|preset| preset.has_structured_chat())?;
            let messages = history_of(&(source.messages)(id)?);
            let seed = (!messages.is_empty()).then(|| {
                fork_seed_prompt(
                    &node.title,
                    &source.acp.harness_name(harness),
                    last_turns(&messages),
                )
            });
            let transcript = node
                .resume
                .clone()
                .filter(|_| harness == AgentPreset::ClaudeCode)
                .map(|resume| Transcript {
                    link: source.link.clone(),
                    root: root.clone(),
                    resume,
                });
            let harness = cloud_chat_harness(harness, &candidates);
            Some(CarriedChat {
                terminal: id,
                transcript: transcript.filter(|_| harness == AgentPreset::ClaudeCode),
                harness,
                seed,
                prompts: Vec::new(),
            })
        })
        .collect()
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
    fn a_prompt_sent_with_a_moving_session_follows_its_own_chat_after_the_summary() {
        let chat = |terminal| CarriedChat {
            terminal: TerminalId(terminal),
            harness: AgentPreset::Codex,
            transcript: None,
            seed: Some("earlier".to_owned()),
            prompts: Vec::new(),
        };
        let mut carried = vec![chat(4), chat(5)];
        let prompt = Prompt::new("Fix the flaky test", Vec::new());
        with_sent_prompt(
            &mut carried,
            SentPrompt {
                chat: TerminalId(5),
                prompt: prompt.clone(),
            },
        );
        with_sent_prompt(
            &mut carried,
            SentPrompt {
                chat: TerminalId(9),
                prompt: prompt.clone(),
            },
        );
        let moved = carry_chats(&HostLink::Local, Path::new("/work"), carried);
        let prompts: Vec<(TerminalId, usize)> = moved
            .iter()
            .map(|chat| (chat.terminal, chat.start.prompts.len()))
            .collect();
        assert_eq!(prompts, [(TerminalId(4), 1), (TerminalId(5), 2)]);
        assert_eq!(moved[1].start.prompts.last(), Some(&prompt));
    }

    #[test]
    fn a_chat_that_cannot_be_copied_starts_with_its_summary() {
        let start = fresh_start(AgentPreset::Codex, Some("earlier".to_owned()));
        assert!(matches!(start.conversation, CarriedConversation::Fresh(_)));
        assert_eq!(start.prompts.len(), 1);
        assert_eq!(fresh_start(AgentPreset::Codex, None).prompts.len(), 0);
    }

    #[test]
    fn a_first_chat_starts_fresh_with_its_prompt_on_a_harness_the_runner_runs() {
        let prompt = Prompt::new("Fix the flaky test", Vec::new());
        let first = FirstChat {
            harness: AgentPreset::Codex,
            prompt: prompt.clone(),
        };
        let start = first_start(first, &[AgentPreset::ClaudeCode]);
        assert_eq!(start.harness, AgentPreset::Codex);
        assert!(matches!(start.conversation, CarriedConversation::Fresh(_)));
        assert_eq!(start.prompts, vec![prompt]);
        let shell = FirstChat {
            harness: AgentPreset::Shell,
            prompt: Prompt::new("ls", Vec::new()),
        };
        assert_eq!(
            first_start(shell, &[AgentPreset::ClaudeCode]).harness,
            AgentPreset::ClaudeCode
        );
    }
}
