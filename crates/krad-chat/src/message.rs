use krad_domain::{Effort, ModelId, Prompt, RewindId};
use serde::Serialize;
use std::fmt;

use crate::control::json_line;
use crate::mention::{read_prompt, shown_path, EncodedImage};

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
    User {
        message: UserMessage<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        uuid: Option<&'a RewindId>,
    },
}

#[derive(Serialize)]
struct UserMessage<'a> {
    role: Role,
    content: Vec<ContentBlock<'a>>,
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
    Image { source: ImageSource<'a> },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ImageSource<'a> {
    Base64 {
        media_type: &'static str,
        data: &'a str,
    },
}

fn content_line(text: &str, images: &[EncodedImage], uuid: Option<&RewindId>) -> String {
    let text = (!text.is_empty() || images.is_empty()).then_some(ContentBlock::Text { text });
    let content = text
        .into_iter()
        .chain(images.iter().map(|image| ContentBlock::Image {
            source: ImageSource::Base64 {
                media_type: image.media_type,
                data: &image.data,
            },
        }))
        .collect();
    json_line(&InputLine::User {
        message: UserMessage {
            role: Role::User,
            content,
        },
        uuid,
    })
}

pub(crate) fn user_message_line(prompt: &Prompt, uuid: Option<&RewindId>) -> String {
    let read = read_prompt(prompt, shown_path);
    content_line(&read.text, &read.images, uuid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn a_message_is_one_stream_json_user_line() {
        let line = user_message_line(&Prompt::from("say \"hi\"\nthen stop"), None);
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
    fn an_image_goes_as_a_base64_block_after_the_text() {
        let image = EncodedImage {
            media_type: "image/png",
            data: "cG5n".to_owned(),
        };
        let line = content_line("What is this?", std::slice::from_ref(&image), None);
        let value: Value = serde_json::from_str(&line).unwrap_or_default();
        assert_eq!(
            value["message"]["content"],
            json!([
                { "type": "text", "text": "What is this?" },
                {
                    "type": "image",
                    "source": { "type": "base64", "media_type": "image/png", "data": "cG5n" },
                },
            ])
        );
        let alone: Value =
            serde_json::from_str(&content_line("", &[image], None)).unwrap_or_default();
        assert_eq!(alone["message"]["content"][0]["type"], "image");
        assert_eq!(
            alone["message"]["content"].as_array().map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn a_prompt_carries_the_uuid_claude_code_keeps_as_its_message_id() {
        let uuid = RewindId::from("aaaaaaaa-0000-4000-8000-000000000001");
        let line = user_message_line(&Prompt::from("hi"), Some(&uuid));
        let value: Value = serde_json::from_str(&line).unwrap_or_default();
        assert_eq!(value["uuid"], "aaaaaaaa-0000-4000-8000-000000000001");
        assert_eq!(value["message"]["content"][0]["text"], "hi");
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
