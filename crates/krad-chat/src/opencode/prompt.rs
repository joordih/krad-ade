use krad_domain::{Mention as Attached, Prompt};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

use crate::mention::read_image;

const MENTION_MARK: char = '@';
const COMMAND_MARK: char = '/';
const FILE_SCHEME: &str = "file://";
const PREAMBLE_OPEN: &str = "<system-reminder>\n";
const PREAMBLE_CLOSE: &str = "\n</system-reminder>\n\n";
const URI_KEPT: &[u8] = b"-._~/";
const URI_SEPARATOR: char = '/';
const WINDOWS_SEPARATOR: char = '\\';
const TRAILING_PUNCTUATION: &[char] = &[',', '.', ';', ':', '!', '?', ')', ']'];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Delivery {
    Queue,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Mention {
    start: usize,
    end: usize,
    text: String,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct FilePart {
    uri: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mention: Option<Mention>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct SkillPart {
    id: String,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PromptBody {
    text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    files: Vec<FilePart>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skills: Vec<SkillPart>,
    delivery: Delivery,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct CommandBody {
    name: String,
    text: String,
    delivery: Delivery,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outgoing {
    Prompt(PromptBody),
    Command(CommandBody),
}

pub(crate) struct Known<'a> {
    pub(crate) commands: &'a HashSet<String>,
    pub(crate) skills: &'a HashSet<String>,
}

pub(crate) struct Place<'a> {
    pub(crate) local: &'a Path,
    pub(crate) served: &'a str,
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn file_uri(served: &str, relative: &str) -> String {
    encoded_uri(&format!(
        "{}/{relative}",
        served.trim_end_matches(COMMAND_MARK)
    ))
}

fn path_uri(path: &Path) -> String {
    let path = path
        .to_string_lossy()
        .replace(WINDOWS_SEPARATOR, &URI_SEPARATOR.to_string());
    if path.starts_with(URI_SEPARATOR) {
        encoded_uri(&path)
    } else {
        encoded_uri(&format!("{URI_SEPARATOR}{path}"))
    }
}

fn encoded_uri(path: &str) -> String {
    let encoded: String = path
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || URI_KEPT.contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    format!("{FILE_SCHEME}{encoded}")
}

fn mentioned_files(text: &str, place: &Place<'_>) -> Vec<FilePart> {
    let mut files = Vec::new();
    let mut offset = 0;
    for word in text.split_inclusive(char::is_whitespace) {
        let start = offset;
        offset += utf16_len(word);
        let Some(mentioned) = word.trim_end().strip_prefix(MENTION_MARK) else {
            continue;
        };
        let relative = mentioned.trim_end_matches(TRAILING_PUNCTUATION);
        if relative.is_empty() || !place.local.join(relative).exists() {
            continue;
        }
        let shown = format!("{MENTION_MARK}{relative}");
        files.push(FilePart {
            uri: file_uri(place.served, relative),
            name: Path::new(relative).file_name().map_or_else(
                || relative.to_owned(),
                |name| name.to_string_lossy().into_owned(),
            ),
            mention: Some(Mention {
                start,
                end: start + utf16_len(&shown),
                text: shown,
            }),
        });
    }
    files
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

pub(crate) fn attachments(prompt: &Prompt) -> Vec<FilePart> {
    prompt
        .mentions
        .iter()
        .map(|mention| {
            let uri = match mention {
                Attached::Image(image) => read_image(image)
                    .map_or_else(|_| path_uri(&image.path), |encoded| encoded.data_url()),
                Attached::Path(path) => path_uri(path),
            };
            FilePart {
                uri,
                name: file_name(mention.path()),
                mention: None,
            }
        })
        .collect()
}

pub(crate) fn strip_preamble(text: &str) -> &str {
    text.strip_prefix(PREAMBLE_OPEN)
        .and_then(|rest| rest.split_once(PREAMBLE_CLOSE))
        .map_or(text, |(_, message)| message)
}

fn with_preamble(text: &str, preamble: Option<&str>) -> String {
    match preamble {
        Some(preamble) => format!("{PREAMBLE_OPEN}{preamble}{PREAMBLE_CLOSE}{text}"),
        None => text.to_owned(),
    }
}

pub(crate) fn outgoing(
    text: &str,
    preamble: Option<&str>,
    known: &Known<'_>,
    place: &Place<'_>,
    attached: Vec<FilePart>,
) -> Outgoing {
    let trimmed = text.trim();
    let called = trimmed.strip_prefix(COMMAND_MARK).map(|call| {
        call.split_once(char::is_whitespace)
            .map_or((call, ""), |(name, rest)| (name, rest.trim()))
    });
    if let Some((name, rest)) = called.filter(|(name, _)| known.commands.contains(*name)) {
        if preamble.is_none() && attached.is_empty() {
            return Outgoing::Command(CommandBody {
                name: name.to_owned(),
                text: rest.to_owned(),
                delivery: Delivery::Queue,
            });
        }
    }
    let (text, skills) = match called.filter(|(name, _)| known.skills.contains(*name)) {
        Some((name, rest)) => (
            if rest.is_empty() { name } else { rest },
            vec![SkillPart {
                id: name.to_owned(),
            }],
        ),
        None => (trimmed, Vec::new()),
    };
    let text = with_preamble(text, preamble);
    let mut files = mentioned_files(&text, place);
    files.extend(attached);
    Outgoing::Prompt(PromptBody {
        files,
        text,
        skills,
        delivery: Delivery::Queue,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sets() -> (HashSet<String>, HashSet<String>) {
        (
            HashSet::from(["review".to_owned()]),
            HashSet::from(["git-release".to_owned()]),
        )
    }

    #[test]
    fn a_mention_of_a_file_that_exists_becomes_an_attachment() {
        let root = std::env::temp_dir().join("krad-opencode-mentions");
        fs::create_dir_all(root.join("src"))
            .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        fs::write(root.join("src/my file.rs"), "")
            .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        fs::write(root.join("src/a.rs"), "")
            .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        let (commands, skills) = sets();
        let known = Known {
            commands: &commands,
            skills: &skills,
        };
        let place = Place {
            local: &root,
            served: "/srv/proj",
        };
        let Outgoing::Prompt(body) = outgoing(
            "Ñu explain @src/a.rs, not @missing.rs",
            None,
            &known,
            &place,
            Vec::new(),
        ) else {
            unreachable!("a plain message is a prompt");
        };
        assert_eq!(
            body.files,
            [FilePart {
                uri: "file:///srv/proj/src/a.rs".to_owned(),
                name: "a.rs".to_owned(),
                mention: Some(Mention {
                    start: 11,
                    end: 20,
                    text: "@src/a.rs".to_owned(),
                }),
            }]
        );
        assert_eq!(
            file_uri("/srv/proj/", "src/my file.rs"),
            "file:///srv/proj/src/my%20file.rs"
        );
    }

    #[test]
    fn a_known_command_runs_as_a_command_and_a_skill_is_attached() {
        let (commands, skills) = sets();
        let known = Known {
            commands: &commands,
            skills: &skills,
        };
        let place = Place {
            local: Path::new("/nowhere"),
            served: "/nowhere",
        };
        assert_eq!(
            outgoing("/review src/auth.rs", None, &known, &place, Vec::new()),
            Outgoing::Command(CommandBody {
                name: "review".to_owned(),
                text: "src/auth.rs".to_owned(),
                delivery: Delivery::Queue,
            })
        );
        let Outgoing::Prompt(body) = outgoing("/git-release v2", None, &known, &place, Vec::new())
        else {
            unreachable!("a skill goes with a prompt");
        };
        assert_eq!(body.text, "v2");
        assert_eq!(
            body.skills,
            [SkillPart {
                id: "git-release".to_owned()
            }]
        );
        let Outgoing::Prompt(body) = outgoing("/unknown thing", None, &known, &place, Vec::new())
        else {
            unreachable!("an unknown command is plain text");
        };
        assert_eq!(body.text, "/unknown thing");
    }

    #[test]
    fn the_first_message_carries_the_app_instructions() {
        let (commands, skills) = sets();
        let known = Known {
            commands: &commands,
            skills: &skills,
        };
        let place = Place {
            local: Path::new("/nowhere"),
            served: "/nowhere",
        };
        let Outgoing::Prompt(body) =
            outgoing("hi", Some("Use the bus."), &known, &place, Vec::new())
        else {
            unreachable!("a message with instructions is a prompt");
        };
        assert_eq!(
            body.text,
            "<system-reminder>\nUse the bus.\n</system-reminder>\n\nhi"
        );
    }

    #[test]
    fn a_picked_file_goes_by_its_uri_and_an_image_inline_as_a_data_url() {
        let dir = std::env::temp_dir().join("krad-opencode-attachments");
        fs::create_dir_all(&dir).unwrap_or_else(|_| unreachable!("the fixture is valid"));
        let shot = dir.join("1.png");
        fs::write(&shot, b"png").unwrap_or_else(|_| unreachable!("the fixture is valid"));
        let prompt = Prompt::new(
            "Compare",
            vec![
                Attached::from(std::path::PathBuf::from("/repo/my notes.md")),
                Attached::from(shot),
            ],
        );
        let files = attachments(&prompt);
        assert_eq!(
            files,
            [
                FilePart {
                    uri: "file:///repo/my%20notes.md".to_owned(),
                    name: "my notes.md".to_owned(),
                    mention: None,
                },
                FilePart {
                    uri: "data:image/png;base64,cG5n".to_owned(),
                    name: "1.png".to_owned(),
                    mention: None,
                },
            ]
        );
        assert_eq!(
            serde_json::to_value(&files[1]).unwrap_or_default(),
            serde_json::json!({"uri": "data:image/png;base64,cG5n", "name": "1.png"})
        );
        let (commands, skills) = sets();
        let known = Known {
            commands: &commands,
            skills: &skills,
        };
        let place = Place {
            local: Path::new("/nowhere"),
            served: "/nowhere",
        };
        let Outgoing::Prompt(body) = outgoing("/review", None, &known, &place, files) else {
            unreachable!("a command with attachments goes as a prompt");
        };
        assert_eq!(body.files.len(), 2);
    }
}
