mod changes;
mod detail;
mod diff;
mod header;
mod host;
mod list;
mod marks;
mod offer;
mod overview;
mod remote;
mod summary;
mod tabs;
mod threads;
mod words;

use gpui_kit::{div, AnyElement, App, Context, IntoElement, ParentElement, Styled};
use krad_domain::review::{
    CiStatus, MergeRequest, MergeRequestIid, MergeRequestQuery, ProjectPath, Scope, StateFilter,
    User, UserId,
};
use krad_domain::CodeHost;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::settings_view::SettingsPane;
use super::EnvironmentShell;
use crate::components::{
    card, split_island, Button, ButtonVariant, CenteredEmpty, EmptyState, IconName,
};
use crate::theme::{palette, Palette};
pub(super) use changes::diff_row;
use detail::RequestDetail;
pub(super) use diff::{numbered_lines, DiffLine, LineKind};
pub(super) use host::HostClient;
use marks::{
    avatar, ci_color, ci_glyph, ci_label, merge_status_label, meta_line, state_color, state_label,
    status_pill, WASH_ALPHA,
};
use offer::Offer;
use words::{host_words, HostWords};

const BTN_OPEN_SETTINGS: &str = "review-open-settings";
const NO_GROUP_TITLE: &str = "No group selected";
const NO_GROUP_TEXT: &str =
    "Review lists the merge requests of a group's GitLab or GitHub project. Pick a group in the sidebar.";
const NONE_OPEN_TEXT: &str = "Pick one in the list to read its changes and review it.";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum ReviewFilter {
    #[default]
    Open,
    Merged,
    Mine,
    ReviewerMe,
    Closed,
}

impl ReviewFilter {
    const ALL: [Self; 5] = [
        Self::Open,
        Self::Merged,
        Self::Mine,
        Self::ReviewerMe,
        Self::Closed,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Merged => "Merged",
            Self::Mine => "Mine",
            Self::ReviewerMe => "Reviewer: me",
            Self::Closed => "Closed",
        }
    }

    fn counted_label(self, listed: Option<usize>) -> String {
        match (self, listed) {
            (Self::Open, Some(count)) => format!("{} {count}", self.label()),
            _ => self.label().to_owned(),
        }
    }

    fn query(self, me: UserId) -> MergeRequestQuery {
        let open = MergeRequestQuery::default();
        match self {
            Self::Open => open,
            Self::Mine => MergeRequestQuery {
                scope: Scope::CreatedByMe,
                ..open
            },
            Self::ReviewerMe => MergeRequestQuery {
                reviewer: Some(me),
                ..open
            },
            Self::Merged => MergeRequestQuery {
                state: StateFilter::Merged,
                ..open
            },
            Self::Closed => MergeRequestQuery {
                state: StateFilter::Closed,
                ..open
            },
        }
    }
}

#[derive(Default)]
pub(super) enum Remote<T> {
    #[default]
    Loading,
    Failed(String),
    Ready(T),
}

struct Account {
    client: Arc<HostClient>,
    me: User,
}

#[derive(Default)]
enum Connection {
    #[default]
    Idle,
    Connecting,
    SignedOut(String),
    Failed(String),
    Ready(Account),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct RowFacts {
    pipeline: Option<CiStatus>,
    approvals: (usize, usize),
}

struct RequestList {
    root: PathBuf,
    filter: ReviewFilter,
    requests: Remote<(ProjectPath, Vec<MergeRequest>)>,
    facts: HashMap<MergeRequestIid, RowFacts>,
}

#[derive(Default)]
pub(super) struct ReviewUi {
    connection: Connection,
    filter: ReviewFilter,
    list: Option<RequestList>,
    detail: Option<RequestDetail>,
    offers: HashMap<PathBuf, Offer>,
}

impl ReviewUi {
    pub(super) fn forget_login(&mut self) {
        *self = Self {
            filter: self.filter,
            ..Self::default()
        };
    }
}

impl EnvironmentShell {
    pub(super) fn review_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Some(root) = self.active_group().map(|group| group.root.clone()) else {
            return island(
                pal,
                EmptyState::new(IconName::Merge, NO_GROUP_TITLE, NO_GROUP_TEXT),
                cx,
            );
        };
        let stale = match &self.review.connection {
            Connection::Idle => true,
            Connection::Ready(account) => self
                .review_host()
                .is_some_and(|host| host != account.client.code_host()),
            Connection::Connecting | Connection::SignedOut(_) | Connection::Failed(_) => false,
        };
        if stale {
            self.review.list = None;
            self.review.detail = None;
            self.connect_host(root.clone(), cx);
        }
        let words = self.review_words();
        let notice = match &self.review.connection {
            Connection::Idle | Connection::Connecting => {
                Some(EmptyState::new(IconName::Loading, words.connecting, "").into_any_element())
            }
            Connection::SignedOut(reason) => Some(Self::signed_out(reason, words, cx)),
            Connection::Failed(reason) => Some(
                EmptyState::new(IconName::CancelCircle, words.unavailable, reason.clone())
                    .into_any_element(),
            ),
            Connection::Ready(_) => None,
        };
        if let Some(notice) = notice {
            return island(pal, notice, cx);
        }
        self.ensure_review_list(&root, cx);
        let detail = match &self.review.detail {
            Some(detail) => self.request_detail(detail, cx),
            None => {
                EmptyState::new(IconName::Merge, words.none_open, NONE_OPEN_TEXT).into_any_element()
            }
        };
        split_island(
            self.request_list(cx),
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .child(detail),
            pal,
            cx,
        )
        .into_any_element()
    }

    fn signed_out(reason: &str, words: &'static HostWords, cx: &mut Context<Self>) -> AnyElement {
        let open = Button::new(BTN_OPEN_SETTINGS)
            .label(words.sign_in)
            .variant(ButtonVariant::Primary)
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.open_settings(window, cx);
                this.select_pane(SettingsPane::Integrations, cx);
            }));
        CenteredEmpty::new(words.sign_in, reason.to_owned())
            .action(open)
            .into_any_element()
    }

    pub(super) fn review_words(&self) -> &'static HostWords {
        let host = match &self.review.connection {
            Connection::Ready(account) => Some(account.client.code_host()),
            _ => self.review_host(),
        };
        host_words(host.unwrap_or(CodeHost::Gitlab))
    }

    fn gitlab_user(&self) -> Option<UserId> {
        match &self.review.connection {
            Connection::Ready(account) => Some(account.me.id),
            _ => None,
        }
    }
}

fn island(pal: &Palette, content: impl IntoElement, cx: &App) -> AnyElement {
    card(pal, cx).size_full().child(content).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: UserId = UserId(7);

    #[test]
    fn open_filters_list_open_merge_requests() {
        for filter in [
            ReviewFilter::Open,
            ReviewFilter::Mine,
            ReviewFilter::ReviewerMe,
        ] {
            assert_eq!(filter.query(ME).state, StateFilter::Opened);
        }
    }

    #[test]
    fn mine_and_reviewer_scope_to_the_user() {
        let mine = ReviewFilter::Mine.query(ME);
        assert_eq!((mine.scope, mine.reviewer), (Scope::CreatedByMe, None));
        let reviewing = ReviewFilter::ReviewerMe.query(ME);
        assert_eq!(
            (reviewing.scope, reviewing.reviewer),
            (Scope::All, Some(ME))
        );
    }

    #[test]
    fn merged_and_closed_list_everyone_in_that_state() {
        assert_eq!(
            ReviewFilter::Merged.query(ME),
            MergeRequestQuery {
                state: StateFilter::Merged,
                ..MergeRequestQuery::default()
            }
        );
        assert_eq!(
            ReviewFilter::Closed.query(ME),
            MergeRequestQuery {
                state: StateFilter::Closed,
                ..MergeRequestQuery::default()
            }
        );
        assert_eq!(ReviewFilter::Open.query(ME), MergeRequestQuery::default());
    }

    #[test]
    fn only_the_open_filter_counts_what_it_lists() {
        assert_eq!(ReviewFilter::Open.counted_label(Some(4)), "Open 4");
        assert_eq!(ReviewFilter::Open.counted_label(None), "Open");
        assert_eq!(ReviewFilter::Merged.counted_label(Some(9)), "Merged");
        assert_eq!(
            ReviewFilter::ReviewerMe.counted_label(Some(2)),
            "Reviewer: me"
        );
    }
}
