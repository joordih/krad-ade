mod query;

use super::{IssueNumber, MergeRequestIid, MergeRequestState, NoteId, User};

pub use query::{sort_issues, IssueQuery, IssueSort, IssueStateFilter, ListPage};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum IssueState {
    #[default]
    Open,
    Closed,
}

impl IssueState {
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Open => Self::Closed,
            Self::Closed => Self::Open,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Issue {
    pub number: IssueNumber,
    pub title: String,
    pub body: String,
    pub web_url: String,
    pub labels: Vec<String>,
    pub state: IssueState,
    pub author: Option<User>,
    pub assignees: Vec<User>,
    pub milestone: Option<String>,
    pub comments: u32,
    pub reactions: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct IssuePage {
    pub issues: Vec<Issue>,
    pub next: Option<ListPage>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LinkedRequest {
    pub number: MergeRequestIid,
    pub title: String,
    pub state: MergeRequestState,
    pub web_url: String,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct IssueComment {
    pub id: NoteId,
    pub author: User,
    pub body: String,
    pub created_at: String,
    pub reactions: u32,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IssueEventKind {
    Closed,
    Reopened,
    Labeled(String),
    Unlabeled(String),
    Assigned(String),
    Unassigned(String),
    Milestoned(String),
    Demilestoned(String),
    Renamed { from: String, to: String },
    Referenced(LinkedRequest),
    Noted(String),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct IssueEvent {
    pub actor: Option<User>,
    pub kind: IssueEventKind,
    pub created_at: String,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TimelineEntry {
    Comment(IssueComment),
    Event(IssueEvent),
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct IssueActivity {
    pub timeline: Vec<TimelineEntry>,
    pub linked: Vec<LinkedRequest>,
}

impl IssueActivity {
    #[must_use]
    pub fn new(timeline: Vec<TimelineEntry>, related: Vec<LinkedRequest>) -> Self {
        let referenced = timeline.iter().filter_map(|entry| match entry {
            TimelineEntry::Event(IssueEvent {
                kind: IssueEventKind::Referenced(linked),
                ..
            }) => Some(linked.clone()),
            TimelineEntry::Event(_) | TimelineEntry::Comment(_) => None,
        });
        let mut linked: Vec<LinkedRequest> = Vec::new();
        for request in referenced.chain(related) {
            match linked
                .iter_mut()
                .find(|known| known.number == request.number)
            {
                Some(known) => *known = request,
                None => linked.push(request),
            }
        }
        Self { timeline, linked }
    }

    #[must_use]
    pub fn comment_count(&self) -> usize {
        self.timeline
            .iter()
            .filter(|entry| matches!(entry, TimelineEntry::Comment(_)))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::UserId;

    fn linked(number: u64, state: MergeRequestState) -> LinkedRequest {
        LinkedRequest {
            number: MergeRequestIid(number),
            title: format!("Request {number}"),
            state,
            web_url: String::new(),
        }
    }

    fn referenced(number: u64, state: MergeRequestState) -> TimelineEntry {
        TimelineEntry::Event(IssueEvent {
            actor: None,
            kind: IssueEventKind::Referenced(linked(number, state)),
            created_at: String::new(),
        })
    }

    #[test]
    fn linked_requests_come_once_from_references_and_the_related_list() {
        let comment = TimelineEntry::Comment(IssueComment {
            id: NoteId(1),
            author: User {
                id: UserId(1),
                username: "ana".to_owned(),
                name: "Ana".to_owned(),
            },
            body: "On it".to_owned(),
            created_at: String::new(),
            reactions: 2,
        });
        let activity = IssueActivity::new(
            vec![
                referenced(12, MergeRequestState::Opened),
                comment,
                referenced(14, MergeRequestState::Closed),
            ],
            vec![
                linked(12, MergeRequestState::Merged),
                linked(20, MergeRequestState::Opened),
            ],
        );
        let numbers: Vec<(u64, MergeRequestState)> = activity
            .linked
            .iter()
            .map(|request| (request.number.0, request.state))
            .collect();
        assert_eq!(
            numbers,
            [
                (12, MergeRequestState::Merged),
                (14, MergeRequestState::Closed),
                (20, MergeRequestState::Opened)
            ]
        );
        assert_eq!(activity.comment_count(), 1);
    }

    #[test]
    fn closing_and_reopening_flip_the_state() {
        assert_eq!(IssueState::Open.toggled(), IssueState::Closed);
        assert_eq!(IssueState::Closed.toggled(), IssueState::Open);
    }
}
