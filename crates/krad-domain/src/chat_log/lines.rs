use super::{AnswerPart, ChatMessage, Speaker};

const USER_PREFIX: &str = "> ";
const TOOL_PREFIX: &str = "• ";
const ERROR_PREFIX: &str = "Error: ";

#[must_use]
pub fn conversation_lines(messages: &[ChatMessage]) -> Vec<String> {
    messages.iter().flat_map(message_lines).collect()
}

fn message_lines(message: &ChatMessage) -> Vec<String> {
    let mut lines: Vec<String> = match message.speaker {
        Speaker::User => message
            .text
            .lines()
            .map(|line| format!("{USER_PREFIX}{line}"))
            .collect(),
        Speaker::Assistant => message.parts.iter().flat_map(part_lines).collect(),
    };
    lines.extend(
        message
            .error
            .iter()
            .map(|error| format!("{ERROR_PREFIX}{error}")),
    );
    lines
}

fn part_lines(part: &AnswerPart) -> Vec<String> {
    match part {
        AnswerPart::Text(text) => text.lines().map(str::to_owned).collect(),
        AnswerPart::Tool(call) => {
            let line = format!("{TOOL_PREFIX}{} {}", call.name, call.summary);
            vec![line.trim_end().to_owned()]
        }
        AnswerPart::Thinking(_) | AnswerPart::Ask(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::ToolCall;
    use crate::ToolKind;

    fn message(speaker: Speaker, text: &str, parts: Vec<AnswerPart>) -> ChatMessage {
        let mut message = ChatMessage::new(speaker, text.to_owned());
        message.parts = parts;
        message
    }

    #[test]
    fn a_conversation_reads_as_prompts_answers_and_tools_without_thoughts() {
        let tool = ToolCall {
            id: None,
            name: "bash".to_owned(),
            kind: ToolKind::Shell,
            summary: "cargo test".to_owned(),
            edits: Vec::new(),
        };
        let mut answer = message(
            Speaker::Assistant,
            "Ran the tests.\nAll pass.",
            vec![
                AnswerPart::Thinking("hmm".to_owned()),
                AnswerPart::Tool(tool),
                AnswerPart::Text("Ran the tests.\nAll pass.".to_owned()),
            ],
        );
        answer.error = Some("turn stopped".to_owned());
        let lines = conversation_lines(&[
            message(Speaker::User, "Fix it\nquickly", Vec::new()),
            answer,
        ]);
        assert_eq!(
            lines,
            [
                "> Fix it",
                "> quickly",
                "• bash cargo test",
                "Ran the tests.",
                "All pass.",
                "Error: turn stopped",
            ]
        );
    }
}
