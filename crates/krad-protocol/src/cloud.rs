use krad_domain::{
    AgentPreset, CloudRunnerId, EnvironmentName, GroupId, HostedOrigin, Prompt, ResumeId,
    SessionId, TerminalId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CloudDraftId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum PatchAnswer {
    Bring,
    Leave,
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum CloudReach {
    #[default]
    ProcessWire,
    Runtime,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HostedWorkspace {
    pub remote_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_branch: Option<String>,
    pub session_slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HostedSession {
    pub name: String,
    pub workspace: HostedWorkspace,
    #[serde(default)]
    pub starts: Vec<CloudChatStart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<HostedOrigin>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum CloudCommand {
    Create {
        environment: EnvironmentName,
        group: GroupId,
        runner: CloudRunnerId,
        #[serde(default)]
        reach: CloudReach,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        first: Option<FirstChat>,
    },
    Send {
        environment: EnvironmentName,
        session: SessionId,
        runner: CloudRunnerId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<SentPrompt>,
    },
    Host {
        session: Box<HostedSession>,
    },
    AnswerPatch {
        draft: CloudDraftId,
        answer: PatchAnswer,
    },
    Push {
        environment: EnvironmentName,
        session: SessionId,
    },
    OpenRequest {
        environment: EnvironmentName,
        session: SessionId,
    },
    BringHome {
        environment: EnvironmentName,
        session: SessionId,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FirstChat {
    pub harness: AgentPreset,
    pub prompt: Prompt,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SentPrompt {
    pub chat: TerminalId,
    pub prompt: Prompt,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "conversation", content = "resume", rename_all = "snake_case")]
pub enum CarriedConversation {
    Fresh(ResumeId),
    Forked(ResumeId),
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CloudChatStart {
    pub harness: AgentPreset,
    pub conversation: CarriedConversation,
    #[serde(default)]
    pub prompts: Vec<Prompt>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CloudOpened {
    pub environment: EnvironmentName,
    pub session: SessionId,
    pub group: GroupId,
    pub starts: Vec<CloudChatStart>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "news", rename_all = "snake_case")]
pub enum CloudNews {
    AtLimit,
    NoRemote,
    NoSession,
    AlreadyCloud,
    NoGroup,
    NotCloud,
    RunnerNotReady,
    NoRepo,
    RemoteUnread {
        error: String,
    },
    Preparing {
        runner: CloudRunnerId,
    },
    Hosted {
        runner: CloudRunnerId,
        #[serde(default)]
        slug: String,
    },
    RunnerFailed {
        runner: CloudRunnerId,
        reason: String,
    },
    NotPrepared {
        error: String,
    },
    PatchProblem {
        problem: String,
    },
    Pushing,
    Pushed {
        branch: String,
    },
    Opening,
    RequestOpened {
        url: String,
    },
    Bringing,
    BroughtHome {
        branch: String,
    },
    ResultFailed {
        error: String,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloud_command_and_its_news_round_trip() -> serde_json::Result<()> {
        let command = CloudCommand::AnswerPatch {
            draft: CloudDraftId(2),
            answer: PatchAnswer::Bring,
        };
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"verb":"answer_patch","draft":2,"answer":"bring"}"#
        );
        assert_eq!(serde_json::from_str::<CloudCommand>(&json)?, command);
        assert_eq!(
            serde_json::from_str::<CloudCommand>(r#"{"verb":"teleport"}"#)?,
            CloudCommand::Unknown
        );
        let news = CloudNews::RunnerFailed {
            runner: CloudRunnerId(1),
            reason: "no route".to_owned(),
        };
        let json = serde_json::to_string(&news)?;
        assert_eq!(serde_json::from_str::<CloudNews>(&json)?, news);
        Ok(())
    }

    #[test]
    fn a_new_cloud_session_may_carry_its_first_chat_and_an_older_create_reads_without_one(
    ) -> serde_json::Result<()> {
        let older =
            r#"{"verb":"create","environment":"api","group":1,"runner":2,"reach":"runtime"}"#;
        let CloudCommand::Create { first, .. } = serde_json::from_str::<CloudCommand>(older)?
        else {
            unreachable!("a create command");
        };
        assert_eq!(first, None);
        let command = CloudCommand::Create {
            environment: EnvironmentName::new("api".to_owned()),
            group: GroupId(1),
            runner: CloudRunnerId(2),
            reach: CloudReach::Runtime,
            first: Some(FirstChat {
                harness: AgentPreset::Codex,
                prompt: Prompt::new("Fix the flaky test", Vec::new()),
            }),
        };
        let json = serde_json::to_string(&command)?;
        assert_eq!(serde_json::from_str::<CloudCommand>(&json)?, command);
        Ok(())
    }

    #[test]
    fn an_opened_cloud_session_carries_its_chat_starts() -> Result<(), Box<dyn std::error::Error>> {
        let opened = CloudOpened {
            environment: EnvironmentName::new("api".to_owned()),
            session: SessionId(3),
            group: GroupId(1),
            starts: vec![CloudChatStart {
                harness: AgentPreset::ClaudeCode,
                conversation: CarriedConversation::Forked(ResumeId::try_from("abc".to_owned())?),
                prompts: Vec::new(),
            }],
        };
        let json = serde_json::to_string(&opened)?;
        assert_eq!(serde_json::from_str::<CloudOpened>(&json)?, opened);
        Ok(())
    }
}
