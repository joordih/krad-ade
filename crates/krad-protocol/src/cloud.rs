use krad_domain::{
    AgentPreset, CloudRunnerId, EnvironmentName, GroupId, HostedOrigin, Prompt, ResumeId, SessionId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct CloudDraftId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchAnswer {
    Bring,
    Leave,
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudReach {
    #[default]
    ProcessWire,
    Runtime,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HostedWorkspace {
    pub remote_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_branch: Option<String>,
    pub session_slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HostedSession {
    pub name: String,
    pub workspace: HostedWorkspace,
    #[serde(default)]
    pub starts: Vec<CloudChatStart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<HostedOrigin>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum CloudCommand {
    Create {
        environment: EnvironmentName,
        group: GroupId,
        runner: CloudRunnerId,
        #[serde(default)]
        reach: CloudReach,
    },
    Send {
        environment: EnvironmentName,
        session: SessionId,
        runner: CloudRunnerId,
        #[serde(default)]
        reach: CloudReach,
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
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "conversation", content = "resume", rename_all = "snake_case")]
pub enum CarriedConversation {
    Fresh(ResumeId),
    Forked(ResumeId),
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CloudChatStart {
    pub harness: AgentPreset,
    pub conversation: CarriedConversation,
    #[serde(default)]
    pub prompts: Vec<Prompt>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CloudOpened {
    pub environment: EnvironmentName,
    pub session: SessionId,
    pub group: GroupId,
    pub starts: Vec<CloudChatStart>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
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
