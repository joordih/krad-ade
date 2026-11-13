use crate::chat_log::{AnswerPart, ChatMessage, Speaker};
use crate::chat_search::{HistoryMessage, HistorySpeaker};

const BLOCK_SEPARATOR: &str = "\n\n";
const LINE: &str = "\n";

fn said(message: &ChatMessage) -> Vec<&str> {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            AnswerPart::Text(text) => Some(text.as_str()),
            AnswerPart::Thinking(_) | AnswerPart::Tool(_) | AnswerPart::Ask(_) => None,
        })
        .collect()
}

#[must_use]
pub fn history_of(messages: &[ChatMessage]) -> Vec<HistoryMessage> {
    messages
        .iter()
        .filter_map(|message| {
            let (speaker, text) = match message.speaker {
                Speaker::User => (HistorySpeaker::User, message.text.clone()),
                Speaker::Assistant => (HistorySpeaker::Agent, said(message).join(BLOCK_SEPARATOR)),
            };
            (!text.trim().is_empty()).then_some(HistoryMessage { speaker, text })
        })
        .collect()
}

#[must_use]
pub fn final_reply(messages: &[ChatMessage]) -> Option<String> {
    let last = messages
        .iter()
        .rev()
        .find(|message| message.speaker == Speaker::Assistant)?;
    let text = said(last);
    Some(if text.is_empty() {
        last.text.clone()
    } else {
        text.join(LINE)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(speaker: Speaker, text: &str, parts: Vec<AnswerPart>) -> ChatMessage {
        ChatMessage {
            parts,
            ..ChatMessage::new(speaker, text.to_owned())
        }
    }

    #[test]
    fn the_history_keeps_what_was_said_not_tools_or_thoughts() {
        let messages = [
            message(Speaker::User, "Which port?", Vec::new()),
            message(
                Speaker::Assistant,
                "",
                vec![
                    AnswerPart::Thinking("hmm".to_owned()),
                    AnswerPart::Text("Port 8080.".to_owned()),
                    AnswerPart::Text("Set in config.toml.".to_owned()),
                ],
            ),
            message(
                Speaker::Assistant,
                "",
                vec![AnswerPart::Thinking("only".to_owned())],
            ),
        ];
        let history = history_of(&messages);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].speaker, HistorySpeaker::User);
        assert_eq!(history[1].text, "Port 8080.\n\nSet in config.toml.");
        assert_eq!(final_reply(&messages).as_deref(), Some(""));
        assert_eq!(
            final_reply(&messages[..2]).as_deref(),
            Some("Port 8080.\nSet in config.toml.")
        );
    }
}
