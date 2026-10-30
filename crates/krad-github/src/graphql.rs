use krad_domain::review::{Discussion, DiscussionId, Note, NoteId, NotePosition, User, UserId};
use serde::{Deserialize, Serialize};

use crate::GithubError;

const LIST_THREADS: &str = "query($owner: String!, $name: String!, $number: Int!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100, after: $after) {
        pageInfo { hasNextPage endCursor }
        nodes {
          id isResolved path line originalLine diffSide
          comments(first: 100) {
            nodes { databaseId body createdAt author { login ... on User { databaseId name } } }
          }
        }
      }
    }
  }
}";
const REPLY: &str = "mutation($thread: ID!, $body: String!) {
  addPullRequestReviewThreadReply(input: { pullRequestReviewThreadId: $thread, body: $body }) {
    comment { databaseId body createdAt author { login ... on User { databaseId name } } }
  }
}";
const RESOLVE: &str = "mutation($thread: ID!) {
  resolveReviewThread(input: { threadId: $thread }) { thread { id } }
}";
const ERROR_SEPARATOR: &str = "; ";
const UNRESOLVE: &str = "mutation($thread: ID!) {
  unresolveReviewThread(input: { threadId: $thread }) { thread { id } }
}";

#[derive(Serialize)]
pub(crate) struct Request<V> {
    query: &'static str,
    variables: V,
}

#[derive(Deserialize)]
pub(crate) struct Answer<D> {
    pub(crate) data: Option<D>,
    #[serde(default)]
    pub(crate) errors: Vec<GraphqlError>,
}

impl<D> Answer<D> {
    pub(crate) fn into_data(self) -> Result<D, GithubError> {
        let Self { data, errors } = self;
        match data {
            Some(data) if errors.is_empty() => Ok(data),
            _ => Err(GithubError::Graphql(
                errors
                    .into_iter()
                    .map(|error| error.message)
                    .collect::<Vec<_>>()
                    .join(ERROR_SEPARATOR),
            )),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct GraphqlError {
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct ThreadsVariables<'a> {
    owner: &'a str,
    name: &'a str,
    number: u64,
    after: Option<&'a str>,
}

pub(crate) fn list_threads<'a>(
    owner: &'a str,
    name: &'a str,
    number: u64,
    after: Option<&'a str>,
) -> Request<ThreadsVariables<'a>> {
    Request {
        query: LIST_THREADS,
        variables: ThreadsVariables {
            owner,
            name,
            number,
            after,
        },
    }
}

#[derive(Serialize)]
pub(crate) struct ReplyVariables<'a> {
    thread: &'a str,
    body: &'a str,
}

pub(crate) fn reply<'a>(thread: &'a str, body: &'a str) -> Request<ReplyVariables<'a>> {
    Request {
        query: REPLY,
        variables: ReplyVariables { thread, body },
    }
}

#[derive(Serialize)]
pub(crate) struct ThreadVariables<'a> {
    thread: &'a str,
}

pub(crate) fn resolve(thread: &str, resolved: bool) -> Request<ThreadVariables<'_>> {
    Request {
        query: if resolved { RESOLVE } else { UNRESOLVE },
        variables: ThreadVariables { thread },
    }
}

#[derive(Deserialize)]
pub(crate) struct ThreadsData {
    repository: Option<RepositoryData>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryData {
    pull_request: Option<PullRequestData>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestData {
    review_threads: Connection<ThreadNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection<T> {
    #[serde(default)]
    page_info: Option<PageInfo>,
    nodes: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum DiffSide {
    Left,
    Right,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadNode {
    id: String,
    is_resolved: bool,
    path: String,
    line: Option<u32>,
    original_line: Option<u32>,
    diff_side: DiffSide,
    comments: Connection<CommentNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommentNode {
    database_id: Option<u64>,
    body: String,
    created_at: String,
    author: Option<AuthorNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorNode {
    login: String,
    database_id: Option<u64>,
    name: Option<String>,
}

impl From<AuthorNode> for User {
    fn from(author: AuthorNode) -> Self {
        Self {
            id: UserId(author.database_id.unwrap_or_default()),
            name: author
                .name
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| author.login.clone()),
            username: author.login,
        }
    }
}

const GHOST: &str = "ghost";

impl CommentNode {
    fn into_note(self, resolved: bool, position: Option<NotePosition>) -> Note {
        Note {
            id: NoteId(self.database_id.unwrap_or_default()),
            body: self.body,
            author: self.author.map_or_else(
                || User {
                    id: UserId(0),
                    username: GHOST.to_owned(),
                    name: GHOST.to_owned(),
                },
                User::from,
            ),
            created_at: self.created_at,
            system: false,
            resolvable: true,
            resolved,
            position,
        }
    }
}

impl ThreadNode {
    fn into_discussion(self) -> Discussion {
        let line = self.line.or(self.original_line);
        let position = match self.diff_side {
            DiffSide::Right => NotePosition {
                old_path: None,
                new_path: Some(self.path),
                old_line: None,
                new_line: line,
            },
            DiffSide::Left => NotePosition {
                old_path: Some(self.path),
                new_path: None,
                old_line: line,
                new_line: None,
            },
        };
        let resolved = self.is_resolved;
        Discussion {
            id: DiscussionId(self.id),
            individual_note: false,
            notes: self
                .comments
                .nodes
                .into_iter()
                .map(|comment| comment.into_note(resolved, Some(position.clone())))
                .collect(),
        }
    }
}

pub(crate) fn thread_page(data: ThreadsData) -> (Vec<Discussion>, Option<String>) {
    let Some(threads) = data
        .repository
        .and_then(|repository| repository.pull_request)
        .map(|pull| pull.review_threads)
    else {
        return (Vec::new(), None);
    };
    let next = threads
        .page_info
        .filter(|page| page.has_next_page)
        .and_then(|page| page.end_cursor);
    let discussions = threads
        .nodes
        .into_iter()
        .map(ThreadNode::into_discussion)
        .collect();
    (discussions, next)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReplyData {
    add_pull_request_review_thread_reply: ReplyPayload,
}

#[derive(Deserialize)]
struct ReplyPayload {
    comment: CommentNode,
}

impl ReplyData {
    pub(crate) fn into_note(self) -> Note {
        self.add_pull_request_review_thread_reply
            .comment
            .into_note(false, None)
    }
}

#[derive(Deserialize)]
pub(crate) struct Ignored {}

#[cfg(test)]
mod tests {
    use super::*;

    const THREADS: &str = r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{
        "pageInfo":{"hasNextPage":true,"endCursor":"c1"},
        "nodes":[{"id":"PRRT_1","isResolved":true,"path":"src/a.rs","line":12,
          "originalLine":10,"diffSide":"RIGHT","comments":{"nodes":[
            {"databaseId":5,"body":"Why?","createdAt":"2026-10-01T10:00:00Z",
             "author":{"login":"bo","databaseId":2,"name":"Bo"}},
            {"databaseId":6,"body":"Gone","createdAt":"2026-10-01T11:00:00Z","author":null}
          ]}}]}}}}}"#;

    #[test]
    fn review_threads_read_as_resolvable_discussions_on_a_line() -> serde_json::Result<()> {
        let answer: Answer<ThreadsData> = serde_json::from_str(THREADS)?;
        let (discussions, next) = answer.data.map_or_default(thread_page);
        assert_eq!(next.as_deref(), Some("c1"));
        let thread = &discussions[0];
        assert_eq!(thread.id, DiscussionId("PRRT_1".to_owned()));
        assert!(thread.is_resolvable() && thread.is_resolved());
        let position = thread.notes[0].position.as_ref();
        assert_eq!(position.and_then(|at| at.new_line), Some(12));
        assert_eq!(thread.notes[1].author.username, GHOST);
        Ok(())
    }

    fn refusal(json: &str) -> Option<String> {
        let answer: Answer<Ignored> = serde_json::from_str(json).ok()?;
        match answer.into_data() {
            Err(GithubError::Graphql(message)) => Some(message),
            Ok(_) | Err(_) => None,
        }
    }

    #[test]
    fn graphql_errors_refuse_the_request_with_every_message() {
        assert_eq!(
            refusal(
                r#"{"data":null,"errors":[{"message":"Not found"},{"message":"Rate limited"}]}"#
            ),
            Some("Not found; Rate limited".to_owned())
        );
        assert_eq!(
            refusal(r#"{"data":{},"errors":[{"message":"Partly failed"}]}"#),
            Some("Partly failed".to_owned()),
            "an answer with errors is refused even when it carries data"
        );
        assert_eq!(refusal(r#"{"data":null}"#), Some(String::new()));
        let answered: Option<Answer<Ignored>> = serde_json::from_str(r#"{"data":{}}"#).ok();
        assert!(answered.is_some_and(|answer| answer.into_data().is_ok()));
    }
}
