use krad_domain::{Effort, ModelId};
use serde::Serialize;
use std::fmt;

use crate::control::json_line;

const MODEL_COMMAND: &str = "/model";
const EFFORT_COMMAND: &str = "/effort";
const THINKING_COMMAND: &str = "/config thinking=";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlashCommand {
    Model(ModelId),
    Effort(Effort),
    Thinking(bool),
}

impl fmt::Display for SlashCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Model(model) => write!(formatter, "{MODEL_COMMAND} {model}"),
            Self::Effort(effort) => write!(formatter, "{EFFORT_COMMAND} {effort}"),
            Self::Thinking(on) => write!(formatter, "{THINKING_COMMAND}{on}"),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InputLine<'a> {
    User { message: UserMessage<'a> },
}

#[derive(Serialize)]
struct UserMessage<'a> {
    role: Role,
    content: [ContentBlock<'a>; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    User,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock<'a> {
    Text { text: &'a str },
}

pub(crate) fn user_message_line(prompt: &str) -> String {
    json_line(&InputLine::User {
        message: UserMessage {
            role: Role::User,
            content: [ContentBlock::Text { text: prompt }],
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn a_message_is_one_stream_json_user_line() {
        let line = user_message_line("say \"hi\"\nthen stop");
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1, "one line per message");
        let value: Value = serde_json::from_str(&line).unwrap_or_default();
        assert_eq!(
            value,
            json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": [{ "type": "text", "text": "say \"hi\"\nthen stop" }],
                },
            })
        );
    }

    #[test]
    fn settings_read_as_the_commands_claude_code_takes() {
        let Some(model) = ModelId::parse("sonnet") else {
            unreachable!("a valid model id");
        };
        assert_eq!(SlashCommand::Model(model).to_string(), "/model sonnet");
        assert_eq!(
            SlashCommand::Effort(Effort::ExtraHigh).to_string(),
            "/effort xhigh"
        );
        assert_eq!(
            SlashCommand::Thinking(true).to_string(),
            "/config thinking=true"
        );
        assert_eq!(
            SlashCommand::Thinking(false).to_string(),
            "/config thinking=false"
        );
    }
}
