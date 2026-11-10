use super::export::transcript_sections;
use super::fold::{exchange_of, HistoryMessage};
use crate::agent::AgentPreset;

impl AgentPreset {
    #[must_use]
    pub const fn forks(self) -> bool {
        matches!(
            self,
            Self::ClaudeCode | Self::Codex | Self::Opencode | Self::Gateway
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForkPlan {
    Whole,
    Seeded(String),
}

#[must_use]
pub fn fork_seed_prompt(title: &str, harness: &str, messages: &[HistoryMessage]) -> String {
    format!(
        "This chat continues an earlier one, \u{201c}{}\u{201d}, from the point below. Read the \
         conversation so far, keep it as your context, and reply only that you are ready to \
         go on from there.\n{}",
        title.trim(),
        transcript_sections(harness, messages)
    )
}

#[must_use]
pub fn fork_plan(
    title: &str,
    harness: &str,
    messages: &[HistoryMessage],
    message: usize,
    harness_forks: bool,
) -> ForkPlan {
    let end = exchange_of(messages, message).end;
    if harness_forks && end == messages.len() {
        return ForkPlan::Whole;
    }
    let kept = messages.get(..end).unwrap_or(messages);
    ForkPlan::Seeded(fork_seed_prompt(title, harness, kept))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_search::HistorySpeaker;

    fn conversation() -> Vec<HistoryMessage> {
        [
            (HistorySpeaker::User, "first question"),
            (HistorySpeaker::Agent, "first answer"),
            (HistorySpeaker::User, "second question"),
            (HistorySpeaker::Agent, "second answer"),
        ]
        .into_iter()
        .map(|(speaker, text)| HistoryMessage {
            speaker,
            text: text.to_owned(),
        })
        .collect()
    }

    #[test]
    fn only_harnesses_with_their_own_fork_copy_a_whole_conversation() {
        assert!(AgentPreset::ClaudeCode.forks());
        assert!(AgentPreset::Gateway.forks());
        assert!(!AgentPreset::Shell.forks());
    }

    #[test]
    fn the_last_exchange_forks_through_the_harness_when_it_can() {
        let messages = conversation();
        assert_eq!(
            fork_plan("Chat", "Codex", &messages, 2, true),
            ForkPlan::Whole
        );
        assert_eq!(
            fork_plan("Chat", "Codex", &messages, 3, true),
            ForkPlan::Whole
        );
    }

    #[test]
    fn an_earlier_message_seeds_a_new_chat_with_the_conversation_up_to_its_answer() {
        let messages = conversation();
        let plan = fork_plan("Chat", "Codex", &messages, 0, true);
        let ForkPlan::Seeded(seed) = plan else {
            return assert_ne!(plan, ForkPlan::Whole);
        };
        assert!(seed.contains("\u{201c}Chat\u{201d}"));
        assert!(seed.ends_with("\n## You\n\nfirst question\n\n## Codex\n\nfirst answer\n"));
        assert!(!seed.contains("second"));
    }

    #[test]
    fn a_harness_that_cannot_fork_is_seeded_even_at_the_end() {
        let messages = conversation();
        let plan = fork_plan("Chat", "Agent", &messages, 3, false);
        let ForkPlan::Seeded(seed) = plan else {
            return assert_ne!(plan, ForkPlan::Whole);
        };
        assert!(seed.contains("second answer"));
    }
}
