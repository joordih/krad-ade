use std::cmp::Reverse;

use super::{Issue, IssueState};

const NUMBER_PREFIX: char = '#';

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum IssueStateFilter {
    #[default]
    Open,
    Closed,
    All,
}

impl IssueStateFilter {
    #[must_use]
    pub const fn admits(self, state: IssueState) -> bool {
        match self {
            Self::Open => matches!(state, IssueState::Open),
            Self::Closed => matches!(state, IssueState::Closed),
            Self::All => true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum IssueSort {
    #[default]
    Newest,
    Oldest,
    RecentlyUpdated,
    MostCommented,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ListPage(pub u32);

impl ListPage {
    pub const FIRST: Self = Self(1);

    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl Default for ListPage {
    fn default() -> Self {
        Self::FIRST
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct IssueQuery {
    pub state: IssueStateFilter,
    pub author: Option<String>,
    pub assignee: Option<String>,
    pub label: Option<String>,
    pub search: Option<String>,
    pub sort: IssueSort,
}

impl IssueQuery {
    #[must_use]
    pub fn admits(&self, issue: &Issue) -> bool {
        let author = self.author.as_deref().is_none_or(|wanted| {
            issue
                .author
                .as_ref()
                .is_some_and(|author| author.username == wanted)
        });
        let assignee = self.assignee.as_deref().is_none_or(|wanted| {
            issue
                .assignees
                .iter()
                .any(|assignee| assignee.username == wanted)
        });
        let label = self
            .label
            .as_deref()
            .is_none_or(|wanted| issue.labels.iter().any(|label| label == wanted));
        self.state.admits(issue.state) && author && assignee && label && self.finds(issue)
    }

    fn finds(&self, issue: &Issue) -> bool {
        let Some(search) = self.search.as_deref().map(str::trim) else {
            return true;
        };
        if search.is_empty() {
            return true;
        }
        let needle = search.to_lowercase();
        let number = needle.trim_start_matches(NUMBER_PREFIX);
        issue.number.to_string() == number
            || issue.title.to_lowercase().contains(&needle)
            || issue.body.to_lowercase().contains(&needle)
    }
}

pub fn sort_issues(issues: &mut [Issue], sort: IssueSort) {
    match sort {
        IssueSort::Newest => issues.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
        IssueSort::Oldest => issues.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        IssueSort::RecentlyUpdated => issues.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
        IssueSort::MostCommented => issues.sort_by_key(|issue| Reverse(issue.comments)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{IssueNumber, User, UserId};

    fn person(name: &str) -> User {
        User {
            id: UserId(1),
            username: name.to_owned(),
            name: name.to_owned(),
        }
    }

    fn issue(number: u64, created: &str, updated: &str, comments: u32) -> Issue {
        Issue {
            number: IssueNumber(number),
            title: format!("Checkout breaks {number}"),
            body: "Steps to reproduce".to_owned(),
            created_at: created.to_owned(),
            updated_at: updated.to_owned(),
            comments,
            ..Issue::default()
        }
    }

    fn numbers(issues: &[Issue]) -> Vec<u64> {
        issues.iter().map(|issue| issue.number.0).collect()
    }

    #[test]
    fn every_sort_orders_the_page_its_own_way() {
        let mut issues = vec![
            issue(1, "2026-10-01T09:00:00Z", "2026-10-05T09:00:00Z", 2),
            issue(2, "2026-10-03T09:00:00Z", "2026-10-03T09:00:00Z", 9),
            issue(3, "2026-10-02T09:00:00Z", "2026-10-09T09:00:00Z", 0),
        ];
        sort_issues(&mut issues, IssueSort::Newest);
        assert_eq!(numbers(&issues), [2, 3, 1]);
        sort_issues(&mut issues, IssueSort::Oldest);
        assert_eq!(numbers(&issues), [1, 3, 2]);
        sort_issues(&mut issues, IssueSort::RecentlyUpdated);
        assert_eq!(numbers(&issues), [3, 1, 2]);
        sort_issues(&mut issues, IssueSort::MostCommented);
        assert_eq!(numbers(&issues), [2, 1, 3]);
    }

    #[test]
    fn a_query_admits_only_issues_matching_every_filter() {
        let mut bug = issue(7, "", "", 0);
        bug.labels = vec!["bug".to_owned()];
        bug.author = Some(person("ana"));
        bug.assignees = vec![person("bo")];
        let closed = Issue {
            state: IssueState::Closed,
            ..bug.clone()
        };
        let query = |edit: fn(&mut IssueQuery)| {
            let mut query = IssueQuery::default();
            edit(&mut query);
            query
        };
        assert!(IssueQuery::default().admits(&bug));
        assert!(!IssueQuery::default().admits(&closed));
        assert!(query(|q| q.state = IssueStateFilter::All).admits(&closed));
        assert!(query(|q| q.label = Some("bug".to_owned())).admits(&bug));
        assert!(!query(|q| q.label = Some("docs".to_owned())).admits(&bug));
        assert!(query(|q| q.author = Some("ana".to_owned())).admits(&bug));
        assert!(!query(|q| q.author = Some("bo".to_owned())).admits(&bug));
        assert!(query(|q| q.assignee = Some("bo".to_owned())).admits(&bug));
        assert!(query(|q| q.search = Some("CHECKOUT".to_owned())).admits(&bug));
        assert!(query(|q| q.search = Some("#7".to_owned())).admits(&bug));
        assert!(query(|q| q.search = Some("  ".to_owned())).admits(&bug));
        assert!(!query(|q| q.search = Some("refund".to_owned())).admits(&bug));
    }

    #[test]
    fn pages_count_up_from_the_first() {
        assert_eq!(ListPage::default(), ListPage(1));
        assert_eq!(ListPage::FIRST.next(), ListPage(2));
    }
}
