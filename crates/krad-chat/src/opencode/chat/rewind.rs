use krad_domain::RewindPoint;
use serde::Serialize;
use serde_json::Value;

use super::{session_path, Worker};
use crate::rewind::{RewindError, RewindRequest, Rewound};

const STAGE_TAIL: &str = "/revert/stage";
const COMMIT_TAIL: &str = "/revert/commit";

#[derive(Debug, PartialEq, Eq, Serialize)]
struct StagedRevert<'a> {
    #[serde(rename = "messageID")]
    message_id: &'a str,
    files: bool,
}

fn staged_revert(request: &RewindRequest) -> Result<StagedRevert<'_>, RewindError> {
    let RewindPoint::Message(message) = &request.point else {
        return Err(RewindError::UnknownPoint);
    };
    Ok(StagedRevert {
        message_id: message.as_str(),
        files: request.scope.restores_files(),
    })
}

impl Worker {
    pub(super) fn rewind(&mut self, request: &RewindRequest) -> Result<Rewound, RewindError> {
        let staged = staged_revert(request)?;
        let refused = |error: super::OpencodeError| RewindError::Refused(error.to_string());
        self.server
            .post::<_, Value>(&session_path(&self.session, STAGE_TAIL), &staged)
            .map_err(refused)?;
        self.server
            .nudge(&session_path(&self.session, COMMIT_TAIL))
            .map_err(refused)?;
        self.preamble.clone_from(&self.config.system_prompt);
        Ok(Rewound::InPlace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{RewindId, RewindScope};
    use serde_json::json;

    fn request(point: RewindPoint, scope: RewindScope) -> RewindRequest {
        RewindRequest {
            point,
            scope,
            home: None,
        }
    }

    #[test]
    fn a_revert_is_staged_at_the_edited_message_with_or_without_its_files() {
        let message = || RewindPoint::Message(RewindId::from("msg_18db09094001"));
        for (scope, files) in [
            (RewindScope::Conversation, false),
            (RewindScope::ConversationAndFiles, true),
        ] {
            let edited = request(message(), scope);
            assert_eq!(
                staged_revert(&edited).map(|staged| serde_json::to_value(&staged).ok()),
                Ok(Some(
                    json!({"messageID": "msg_18db09094001", "files": files})
                ))
            );
        }
        let turn = request(
            RewindPoint::Turn(RewindId::from("turn_1")),
            RewindScope::Conversation,
        );
        assert_eq!(staged_revert(&turn), Err(RewindError::UnknownPoint));
    }

    #[test]
    fn a_revert_is_staged_and_committed_on_the_session_routes() {
        assert_eq!(
            session_path("ses_1", STAGE_TAIL),
            "/api/session/ses_1/revert/stage"
        );
        assert_eq!(
            session_path("ses_1", COMMIT_TAIL),
            "/api/session/ses_1/revert/commit"
        );
    }
}
