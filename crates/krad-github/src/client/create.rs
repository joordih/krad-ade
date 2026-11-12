use krad_domain::review::{MergeRequest, NewMergeRequest, ProjectPath};
use serde::Serialize;

use super::{repo_path, GithubClient, GithubError};
use crate::model::PullJson;

const PULLS_PATH: &str = "/pulls";

#[derive(Serialize)]
struct PullCreateBody<'a> {
    title: &'a str,
    head: &'a str,
    base: &'a str,
    body: &'a str,
}

impl<'a> From<&'a NewMergeRequest> for PullCreateBody<'a> {
    fn from(request: &'a NewMergeRequest) -> Self {
        Self {
            title: &request.title,
            head: &request.source_branch,
            base: &request.target_branch,
            body: &request.description,
        }
    }
}

impl GithubClient {
    /// # Errors
    ///
    /// See [`GithubClient::current_user`]; GitHub answers 422 when the head
    /// branch is not on GitHub or a pull request for it is already open.
    pub fn create_pull_request(
        &self,
        project: &ProjectPath,
        request: &NewMergeRequest,
    ) -> Result<MergeRequest, GithubError> {
        self.send_json::<_, PullJson>(
            &repo_path(project, PULLS_PATH)?,
            &PullCreateBody::from(request),
        )
        .map(MergeRequest::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_pull_request_posts_its_branches_title_and_body(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let project: ProjectPath = "octo/app".parse()?;
        assert_eq!(repo_path(&project, PULLS_PATH)?, "/repos/octo/app/pulls");
        let request = NewMergeRequest {
            source_branch: "krad/app-a1".to_owned(),
            target_branch: "main".to_owned(),
            title: "Cloud · app".to_owned(),
            description: "- Fix the build".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(PullCreateBody::from(&request))?,
            serde_json::json!({
                "title": "Cloud · app",
                "head": "krad/app-a1",
                "base": "main",
                "body": "- Fix the build"
            })
        );
        Ok(())
    }
}
