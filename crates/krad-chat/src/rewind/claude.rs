use krad_domain::{ResumeId, RewindId};
use serde::Deserialize;
use serde_json::Value;

use super::{ResumeCut, RewindError};
use crate::command::LiveSession;
use crate::history::is_prompt;
use crate::live::new_session_id;

const UUID_KEY: &str = "uuid";
const PARENT_KEY: &str = "parentUuid";
const FILES_NOT_RESTORED: &str = "Claude Code could not restore the files";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TranscriptCut {
    after: Option<RewindId>,
    last: bool,
}

impl TranscriptCut {
    pub(crate) fn session(self, from: ResumeId, prompt: &RewindId) -> LiveSession {
        match self.after {
            None => LiveSession::Start(new_session_id()),
            Some(at) => LiveSession::Cut(ResumeCut {
                from,
                at,
                drops: self.last.then(|| prompt.clone()),
            }),
        }
    }
}

fn uuid_of(value: &Value, key: &str) -> Option<RewindId> {
    value.get(key).and_then(Value::as_str).map(RewindId::from)
}

pub(crate) fn transcript_cut(
    transcript: &str,
    prompt: &RewindId,
) -> Result<TranscriptCut, RewindError> {
    let mut lines = transcript
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok());
    let found = lines
        .by_ref()
        .find(|value| uuid_of(value, UUID_KEY).as_ref() == Some(prompt))
        .ok_or(RewindError::MissingPrompt)?;
    Ok(TranscriptCut {
        after: uuid_of(&found, PARENT_KEY),
        last: !lines.any(|value| is_prompt(&value)),
    })
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilesReply {
    can_rewind: Option<bool>,
    error: Option<String>,
}

pub(crate) fn files_reply(reply: Result<Value, String>) -> Result<(), RewindError> {
    let reply = FilesReply::deserialize(reply.map_err(RewindError::Refused)?).unwrap_or_default();
    if reply.can_rewind == Some(false) {
        return Err(RewindError::Refused(
            reply.error.unwrap_or_else(|| FILES_NOT_RESTORED.to_owned()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TRANSCRIPT: &str = r#"{"type":"user","uuid":"u1","parentUuid":null,"message":{"role":"user","content":[{"type":"text","text":"one"}]}}
{"type":"attachment","uuid":"x1","parentUuid":"u1"}
{"type":"assistant","uuid":"a1","parentUuid":"x1","message":{"content":[{"type":"text","text":"1"}]}}
{"type":"user","uuid":"u2","parentUuid":"a1","message":{"role":"user","content":[{"type":"text","text":"two"}]}}
{"type":"user","uuid":"r2","parentUuid":"u2","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]}}
{"type":"assistant","uuid":"a2","parentUuid":"r2","message":{"content":[{"type":"text","text":"2"}]}}
{"type":"user","uuid":"u3","parentUuid":"a2","message":{"role":"user","content":"three"}}"#;

    fn id(raw: &str) -> RewindId {
        RewindId::from(raw)
    }

    fn resume() -> ResumeId {
        "0b7c3a52-9f1e"
            .parse()
            .unwrap_or_else(|_| unreachable!("a valid resume id"))
    }

    #[test]
    fn a_prompt_is_cut_after_the_entry_before_it_and_drops_only_the_last_turn() {
        assert_eq!(
            transcript_cut(TRANSCRIPT, &id("u2")),
            Ok(TranscriptCut {
                after: Some(id("a1")),
                last: false,
            })
        );
        assert_eq!(
            transcript_cut(TRANSCRIPT, &id("u3")),
            Ok(TranscriptCut {
                after: Some(id("a2")),
                last: true,
            })
        );
        assert_eq!(
            transcript_cut(TRANSCRIPT, &id("gone")),
            Err(RewindError::MissingPrompt)
        );
    }

    #[test]
    fn the_cut_resumes_at_the_kept_entry_or_starts_over_at_the_first_prompt() {
        let Ok(last) = transcript_cut(TRANSCRIPT, &id("u3")) else {
            unreachable!("u3 is in the transcript");
        };
        assert_eq!(
            last.session(resume(), &id("u3")),
            LiveSession::Cut(ResumeCut {
                from: resume(),
                at: id("a2"),
                drops: Some(id("u3")),
            })
        );
        let Ok(earlier) = transcript_cut(TRANSCRIPT, &id("u2")) else {
            unreachable!("u2 is in the transcript");
        };
        assert!(matches!(
            earlier.session(resume(), &id("u2")),
            LiveSession::Cut(ResumeCut { drops: None, .. })
        ));
        let Ok(first) = transcript_cut(TRANSCRIPT, &id("u1")) else {
            unreachable!("u1 is in the transcript");
        };
        assert!(matches!(
            first.session(resume(), &id("u1")),
            LiveSession::Start(_)
        ));
    }

    #[test]
    fn a_files_reply_fails_when_claude_code_cannot_restore_them() {
        assert_eq!(
            files_reply(Ok(json!({"canRewind": true, "skippedLinks": 0}))),
            Ok(())
        );
        assert_eq!(
            files_reply(Ok(json!({"canRewind": false, "error": "No checkpoint"}))),
            Err(RewindError::Refused("No checkpoint".to_owned()))
        );
        assert_eq!(
            files_reply(Err("File rewinding is not enabled.".to_owned())),
            Err(RewindError::Refused(
                "File rewinding is not enabled.".to_owned()
            ))
        );
    }
}
