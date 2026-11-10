use super::digest::{bounded_fact, MAX_DIGEST_FACTS};
use super::pack::without_memory_packs;
use crate::chat_search::{HistoryMessage, HistorySpeaker};
use crate::review::redact_secrets;

pub const NOTHING_TO_KEEP: &str = "NOTHING_TO_KEEP";
pub const DIGEST_TRANSCRIPT_CHARS: usize = 24_000;
const DIGEST_INTRO: &str = "You write the memory of a krad-ade chat. Read the conversation below and list, as short bullet lines starting with \"- \", the lasting facts and decisions worth remembering in later chats: choices made and why, conventions, commands that work, names of things, open questions. Leave out chit-chat, code listings, file contents and anything secret such as keys, tokens or passwords. Do not use any tool and do not change any file.";
const DIGEST_LIMIT_HEAD: &str = "At most";
const DIGEST_LIMIT_TAIL: &str = "lines.";
const DIGEST_QUIET: &str = "If nothing is worth keeping, reply with exactly this word:";
const TITLE_HEADING: &str = "Chat:";
const TRANSCRIPT_HEADING: &str = "Conversation:";
const USER_LABEL: &str = "User: ";
const AGENT_LABEL: &str = "Agent: ";
const EARLIER_CUT: &str = "[earlier messages left out]";
const PARAGRAPH: &str = "\n\n";
const LINE: &str = "\n";
const BULLETS: [char; 3] = ['-', '*', '•'];
const NUMBER_ENDS: [char; 2] = ['.', ')'];

#[must_use]
pub fn redacted(text: &str) -> String {
    text.lines()
        .map(redact_secrets)
        .collect::<Vec<_>>()
        .join(LINE)
}

fn transcript_tail(messages: &[HistoryMessage]) -> (String, bool) {
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0;
    for message in messages.iter().rev() {
        let label = match message.speaker {
            HistorySpeaker::User => USER_LABEL,
            HistorySpeaker::Agent => AGENT_LABEL,
        };
        let said = without_memory_packs(message.text.trim());
        let said = said.trim();
        if said.is_empty() {
            continue;
        }
        let line = format!("{label}{}", redacted(said));
        let cost = line.chars().count();
        if used + cost > DIGEST_TRANSCRIPT_CHARS {
            return (finish(kept), true);
        }
        used += cost;
        kept.push(line);
    }
    (finish(kept), false)
}

fn finish(mut kept: Vec<String>) -> String {
    kept.reverse();
    kept.join(PARAGRAPH)
}

#[must_use]
pub fn digest_prompt(title: &str, messages: &[HistoryMessage]) -> String {
    let (transcript, cut) = transcript_tail(messages);
    let mut text = format!(
        "{DIGEST_INTRO} {DIGEST_LIMIT_HEAD} {MAX_DIGEST_FACTS} {DIGEST_LIMIT_TAIL} {DIGEST_QUIET} {NOTHING_TO_KEEP}"
    );
    text.push_str(PARAGRAPH);
    text.push_str(TITLE_HEADING);
    text.push(' ');
    text.push_str(&redacted(title));
    text.push_str(PARAGRAPH);
    text.push_str(TRANSCRIPT_HEADING);
    text.push_str(PARAGRAPH);
    if cut {
        text.push_str(EARLIER_CUT);
        text.push_str(PARAGRAPH);
    }
    text.push_str(&transcript);
    text
}

fn bullet_text(line: &str) -> Option<&str> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix(BULLETS) {
        return Some(rest);
    }
    let digits = line.find(|char: char| !char.is_ascii_digit())?;
    (digits > 0)
        .then(|| line[digits..].strip_prefix(NUMBER_ENDS))
        .flatten()
}

#[must_use]
pub fn facts_from_reply(reply: &str) -> Vec<String> {
    if reply.trim().trim_matches('`') == NOTHING_TO_KEEP {
        return Vec::new();
    }
    reply
        .lines()
        .filter_map(bullet_text)
        .filter_map(|fact| bounded_fact(&redacted(fact)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::pack::{MEMORY_PACK_CLOSE, MEMORY_PACK_OPEN};
    use super::*;

    fn message(speaker: HistorySpeaker, text: &str) -> HistoryMessage {
        HistoryMessage {
            speaker,
            text: text.to_owned(),
        }
    }

    #[test]
    fn the_prompt_carries_the_redacted_conversation_and_the_quiet_word() {
        let messages = [
            message(
                HistorySpeaker::User,
                "Deploy with token=ghp_abcdefghijklmnop1234",
            ),
            message(HistorySpeaker::Agent, "Done, it runs on port 8080."),
        ];
        let prompt = digest_prompt("Deploys", &messages);
        assert!(prompt.contains(NOTHING_TO_KEEP));
        assert!(prompt.contains("Chat: Deploys"));
        assert!(
            prompt.contains("User: Deploy with token=[REDACTED]"),
            "{prompt}"
        );
        assert!(prompt.ends_with("Agent: Done, it runs on port 8080."));
        assert!(!prompt.contains(EARLIER_CUT));
    }

    #[test]
    fn a_digest_never_carries_the_memory_packs_the_chat_was_given() {
        let pack = format!(
            "{MEMORY_PACK_OPEN}\nMemory of the Kraddie B, attached by the user:\n- [kraddie/b · keepsake · kraddie/b/keepsake/1] B rotates the staging password\n{MEMORY_PACK_CLOSE}"
        );
        let messages = [
            message(HistorySpeaker::User, &pack),
            message(
                HistorySpeaker::User,
                &format!("Ship the release $b\n\n{pack}"),
            ),
            message(HistorySpeaker::Agent, "Released 1.2."),
        ];
        let prompt = digest_prompt("Release", &messages);
        assert!(!prompt.contains("staging password"), "{prompt}");
        assert!(!prompt.contains(MEMORY_PACK_OPEN));
        assert!(prompt.contains("User: Ship the release $b"));
        assert!(prompt.ends_with("Agent: Released 1.2."));
    }

    #[test]
    fn a_long_conversation_keeps_its_latest_messages() {
        let old = "x".repeat(DIGEST_TRANSCRIPT_CHARS);
        let messages = [
            message(HistorySpeaker::User, &old),
            message(HistorySpeaker::Agent, "latest answer"),
        ];
        let prompt = digest_prompt("Long", &messages);
        assert!(prompt.contains(EARLIER_CUT));
        assert!(prompt.contains("latest answer"));
        assert!(!prompt.contains(&old));
    }

    #[test]
    fn bullets_become_facts_and_secrets_never_survive() {
        let reply = "Here is what I kept:\n- Uses sqlx for the database\n* password: hunter2hunter2\n2. Tests run with `just test`\n\nnot a bullet";
        assert_eq!(
            facts_from_reply(reply),
            [
                "Uses sqlx for the database",
                "password: [REDACTED]",
                "Tests run with `just test`"
            ]
        );
        assert_eq!(facts_from_reply("NOTHING_TO_KEEP").len(), 0);
        assert_eq!(facts_from_reply("`NOTHING_TO_KEEP`").len(), 0);
    }
}
