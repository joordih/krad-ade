use krad_domain::review::{MergeRequest, NewMergeRequest, ProjectPath};
use serde::Serialize;

use super::{project_path, GitlabClient, GitlabError, MERGE_REQUESTS_PATH};
use crate::model::MergeRequestJson;

#[derive(Serialize)]
struct MergeRequestCreateBody<'a> {
    source_branch: &'a str,
    target_branch: &'a str,
    title: &'a str,
    description: &'a str,
}

impl<'a> From<&'a NewMergeRequest> for MergeRequestCreateBody<'a> {
    fn from(request: &'a NewMergeRequest) -> Self {
        Self {
            source_branch: &request.source_branch,
            target_branch: &request.target_branch,
            title: &request.title,
            description: &request.description,
        }
    }
}

impl GitlabClient {
    /// # Errors
    ///
    /// See [`GitlabClient::current_user`]; GitLab answers 409 when a merge
    /// request for the source branch is already open.
    pub fn create_merge_request(
        &self,
        project: &ProjectPath,
        request: &NewMergeRequest,
    ) -> Result<MergeRequest, GitlabError> {
        self.post::<MergeRequestJson, _>(
            &project_path(project, MERGE_REQUESTS_PATH),
            &MergeRequestCreateBody::from(request),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_merge_request_posts_to_the_encoded_project() -> Result<(), Box<dyn std::error::Error>>
    {
        let project: ProjectPath = "team/sub/app".parse()?;
        assert_eq!(
            project_path(&project, MERGE_REQUESTS_PATH),
            "/projects/team%2Fsub%2Fapp/merge_requests"
        );
        let request = NewMergeRequest {
            source_branch: "krad/app-a1".to_owned(),
            target_branch: "main".to_owned(),
            title: "Cloud · app".to_owned(),
            description: "- Fix the build".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(MergeRequestCreateBody::from(&request))?,
            serde_json::json!({
                "source_branch": "krad/app-a1",
                "target_branch": "main",
                "title": "Cloud · app",
                "description": "- Fix the build"
            })
        );
        Ok(())
    }
}
