use super::ApiError;
use krad_domain::WhenWorking;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseTarget {
    Agent(u64),
    Session(u64),
    AllIdle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeFate {
    #[default]
    Keep,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CloseOrder {
    pub target: CloseTarget,
    #[serde(default)]
    pub when_working: WhenWorking,
    #[serde(default)]
    pub worktree: WorktreeFate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosedTarget {
    Agent(u64),
    Session(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeOutcome {
    Kept,
    Removed,
    Refused(ApiError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeLeft {
    pub path: PathBuf,
    pub outcome: WorktreeOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Closed {
    pub target: ClosedTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeLeft>,
}

#[cfg(test)]
mod tests {
    use super::super::{lead_refusal, ErrorCode, Request, Response, TerminalState, WinnerOutcome};
    use super::*;
    use crate::handle::{Caller, LeadHandle, TerminalHandle};

    fn order(target: CloseTarget) -> CloseOrder {
        CloseOrder {
            target,
            when_working: WhenWorking::Refuse,
            worktree: WorktreeFate::Keep,
        }
    }

    #[test]
    fn close_orders_keep_their_shape_on_the_wire() -> Result<(), serde_json::Error> {
        let cases = [
            (
                Request::Close(order(CloseTarget::Agent(5))),
                r#"{"Close":{"target":{"agent":5},"when_working":"refuse","worktree":"keep"}}"#,
            ),
            (
                Request::Close(CloseOrder {
                    target: CloseTarget::Session(3),
                    when_working: WhenWorking::CancelTurn,
                    worktree: WorktreeFate::Remove,
                }),
                r#"{"Close":{"target":{"session":3},"when_working":"cancel_turn","worktree":"remove"}}"#,
            ),
            (
                Request::Close(order(CloseTarget::AllIdle)),
                r#"{"Close":{"target":"all_idle","when_working":"refuse","worktree":"keep"}}"#,
            ),
            (Request::ListAll, r#""ListAll""#),
        ];
        for (request, line) in cases {
            assert_eq!(serde_json::to_string(&request)?, line);
            assert_eq!(serde_json::from_str::<Request>(line)?, request);
        }
        let terse = r#"{"Close":{"target":{"agent":5}}}"#;
        assert_eq!(
            serde_json::from_str::<Request>(terse)?,
            Request::Close(order(CloseTarget::Agent(5)))
        );
        Ok(())
    }

    #[test]
    fn a_close_answer_names_what_closed_and_where_its_worktree_is() -> Result<(), serde_json::Error>
    {
        let answer = Response::Closed(vec![
            Closed {
                target: ClosedTarget::Agent(5),
                worktree: Some(WorktreeLeft {
                    path: PathBuf::from("/src/app.worktrees/fix"),
                    outcome: WorktreeOutcome::Refused(ApiError::new(
                        ErrorCode::WorktreeNotClean,
                        "uncommitted changes",
                    )),
                }),
            },
            Closed {
                target: ClosedTarget::Session(3),
                worktree: None,
            },
        ]);
        let line = serde_json::to_string(&answer)?;
        assert!(line.contains(r#""code":"worktree_not_clean""#), "{line}");
        assert!(line.contains(r#"{"target":{"session":3}}"#), "{line}");
        assert_eq!(serde_json::from_str::<Response>(&line)?, answer);
        assert_eq!(
            serde_json::to_string(&TerminalState::Closed)?,
            r#""closed""#
        );
        assert_eq!(
            serde_json::from_str::<TerminalState>(r#""closed""#)?,
            TerminalState::Closed
        );
        Ok(())
    }

    #[test]
    fn the_close_refusals_have_distinct_stable_codes() -> Result<(), serde_json::Error> {
        for (code, name) in [
            (ErrorCode::NotFound, "not_found"),
            (ErrorCode::NotOwned, "not_owned"),
            (ErrorCode::Busy, "busy"),
            (ErrorCode::Forbidden, "forbidden"),
            (ErrorCode::WorktreeNotClean, "worktree_not_clean"),
        ] {
            assert_eq!(serde_json::to_string(&code)?, format!("\"{name}\""));
        }
        Ok(())
    }

    #[test]
    fn only_a_lead_may_close_and_reading_or_waiting_follows_its_workers() {
        let close = Request::Close(order(CloseTarget::AllIdle));
        let worker = Caller::Terminal(TerminalHandle {
            environment: "main".to_owned(),
            terminal: 4,
        });
        let lead = Caller::Lead(LeadHandle {
            environment: "main".to_owned(),
            session: 2,
        });
        assert!(close.directs_workers());
        assert!(matches!(
            lead_refusal(&worker, &close),
            Some(Response::Error(ApiError {
                code: ErrorCode::Forbidden,
                ..
            }))
        ));
        assert_eq!(lead_refusal(&lead, &close), None);
        let pick = Request::PickVariant {
            run: 1,
            variant: 2,
            outcome: WinnerOutcome::KeepWorktree,
            confirmed: true,
        };
        for destructive in [Request::CloseSession { session: 3 }, pick] {
            assert!(destructive.directs_workers(), "{destructive:?}");
            assert!(matches!(
                lead_refusal(&worker, &destructive),
                Some(Response::Error(ApiError {
                    code: ErrorCode::Forbidden,
                    ..
                }))
            ));
            assert_eq!(lead_refusal(&lead, &destructive), None);
        }
        let read = Request::Read {
            target: 4,
            lines: 10,
        };
        assert_eq!(read.followed_terminal(), Some(4));
        assert!(!read.directs_workers());
        assert_eq!(lead_refusal(&worker, &read), None);
        assert_eq!(Request::List.followed_terminal(), None);
        assert!(close.deadline() > Request::List.deadline());
    }
}
