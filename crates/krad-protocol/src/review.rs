use krad_domain::review::{
    Approvals, Change, CiStatus, Commit, Discussion, DiscussionId, Issue, Job, JobId, MergeOptions,
    MergeRequest, MergeRequestIid, MergeRequestQuery, Pipeline, ProjectPath, User,
};
use krad_domain::{AgentPreset, CodeHost, EnvironmentName, GitlabSignIn, LoginPlace};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::ids::RequestId;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ReviewCommand {
    pub environment: EnvironmentName,
    pub root: PathBuf,
    pub act: ReviewAct,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RequestRef {
    pub project: ProjectPath,
    pub iid: MergeRequestIid,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum ReviewAct {
    Probe,
    Connect,
    List {
        query: MergeRequestQuery,
    },
    Facts {
        project: ProjectPath,
        iids: Vec<MergeRequestIid>,
    },
    Detail {
        request: RequestRef,
    },
    Issues,
    Write {
        request: RequestRef,
        write: ReviewWrite,
    },
    FixJob {
        request: RequestRef,
        job: JobId,
        harness: AgentPreset,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "write", rename_all = "snake_case")]
pub enum ReviewWrite {
    Comment {
        body: String,
    },
    Reply {
        discussion: DiscussionId,
        body: String,
    },
    Resolve {
        discussion: DiscussionId,
    },
    Approve,
    Revoke,
    Merge {
        options: MergeOptions,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "fetched", content = "value", rename_all = "snake_case")]
pub enum Fetched<T> {
    Ready(T),
    Failed(String),
}

impl<T, E: std::fmt::Display> From<Result<T, E>> for Fetched<T> {
    fn from(result: Result<T, E>) -> Self {
        result.map_or_else(|error| Self::Failed(error.to_string()), Self::Ready)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "connection", rename_all = "snake_case")]
pub enum ReviewConnection {
    SignedOut { reason: String },
    Failed { message: String },
    Ready { host: CodeHost, me: User },
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ListedRequests {
    pub project: ProjectPath,
    pub requests: Vec<MergeRequest>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RowFacts {
    pub pipeline: Option<CiStatus>,
    pub approvals: Approvals,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct RequestDetailRead {
    pub iid: MergeRequestIid,
    pub request: Fetched<MergeRequest>,
    pub jobs: Fetched<Vec<Job>>,
    pub commits: Fetched<Vec<Commit>>,
    pub changes: Fetched<Vec<Change>>,
    pub discussions: Fetched<Vec<Discussion>>,
    pub approvals: Fetched<Approvals>,
    pub pipelines: Fetched<Vec<Pipeline>>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "fix", rename_all = "snake_case")]
pub enum FixOutcome {
    Sent {
        branch: String,
    },
    Started {
        branch: String,
    },
    InFork {
        fork: Option<ProjectPath>,
    },
    NoRepository,
    NoBranch {
        branch: String,
    },
    NoChat,
    Failed {
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum ReviewAnswer {
    Probed {
        host: Option<CodeHost>,
    },
    Connected {
        connection: ReviewConnection,
    },
    Listed {
        listed: Fetched<ListedRequests>,
    },
    Facts {
        iid: MergeRequestIid,
        facts: Fetched<RowFacts>,
    },
    Detail {
        detail: Box<RequestDetailRead>,
    },
    Issues {
        issues: Fetched<Vec<Issue>>,
    },
    Written {
        failure: Option<String>,
    },
    Fixed {
        job: JobId,
        fix: FixOutcome,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ReviewAnswered {
    pub request: RequestId,
    pub environment: EnvironmentName,
    pub root: PathBuf,
    pub answer: ReviewAnswer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum CodeHostCommand {
    Check {
        host: CodeHost,
    },
    SignIn {
        with: SignInWith,
    },
    Cancel {
        host: CodeHost,
    },
    SignOut {
        host: CodeHost,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "with", rename_all = "snake_case")]
pub enum SignInWith {
    GithubDevice,
    Gitlab { method: GitlabSignIn },
}

impl SignInWith {
    #[must_use]
    pub const fn host(self) -> CodeHost {
        match self {
            Self::GithubDevice => CodeHost::Github,
            Self::Gitlab { .. } => CodeHost::Gitlab,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "login", rename_all = "snake_case")]
pub enum LoginState {
    Checking,
    SignedOut,
    SignedIn {
        username: Option<String>,
        place: LoginPlace,
    },
    WaitingForBrowser {
        page: String,
    },
    WaitingForCode {
        user_code: String,
        page: String,
        expires_in: Duration,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "news", rename_all = "snake_case")]
pub enum LoginNews {
    SignedIn {
        username: String,
    },
    SignedOut,
    Failed {
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CodeHostLogin {
    pub host: CodeHost,
    pub state: LoginState,
    pub news: Option<LoginNews>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_review_command_and_its_answer_round_trip() -> serde_json::Result<()> {
        let command = ReviewCommand {
            environment: EnvironmentName::new("main".to_owned()),
            root: PathBuf::from("/code/app"),
            act: ReviewAct::Write {
                request: RequestRef {
                    project: "team/app".parse().map_err(serde::de::Error::custom)?,
                    iid: MergeRequestIid(7),
                },
                write: ReviewWrite::Approve,
            },
        };
        let json = serde_json::to_string(&command)?;
        assert_eq!(serde_json::from_str::<ReviewCommand>(&json)?, command);
        let answer = ReviewAnswer::Listed {
            listed: Fetched::Failed("offline".to_owned()),
        };
        let json = serde_json::to_string(&answer)?;
        assert_eq!(serde_json::from_str::<ReviewAnswer>(&json)?, answer);
        assert_eq!(
            serde_json::from_str::<ReviewAct>(r#"{"verb":"later"}"#)?,
            ReviewAct::Unknown
        );
        Ok(())
    }

    #[test]
    fn a_login_waiting_for_its_code_carries_no_token() -> serde_json::Result<()> {
        let login = CodeHostLogin {
            host: CodeHost::Github,
            state: LoginState::WaitingForCode {
                user_code: "ABCD-1234".to_owned(),
                page: "https://github.com/login/device".to_owned(),
                expires_in: Duration::from_mins(15),
            },
            news: None,
        };
        let json = serde_json::to_string(&login)?;
        assert!(!json.contains("token"), "{json}");
        assert_eq!(serde_json::from_str::<CodeHostLogin>(&json)?, login);
        Ok(())
    }
}
