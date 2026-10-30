use krad_domain::RewindPoint;
use serde::Serialize;
use serde_json::Value;

use super::{session_path, Worker};
use crate::rewind::{RewindError, RewindRequest, Rewound};

const STAGE_TAIL: &str = "/revert/stage";
const COMMIT_TAIL: &str = "/revert/commit";

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct StagedRevert<'a> {
    #[serde(rename = "messageID")]
    pub(super) message_id: &'a str,
    pub(super) files: bool,
}

impl Worker {
    pub(super) fn rewind(&mut self, request: &RewindRequest) -> Result<Rewound, RewindError> {
        let RewindPoint::Message(message) = &request.point else {
            return Err(RewindError::UnknownPoint);
        };
        let staged = StagedRevert {
            message_id: message.as_str(),
            files: request.scope.restores_files(),
        };
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
    use serde_json::json;

    #[test]
    fn a_revert_is_staged_at_the_edited_message_with_or_without_its_files() {
        for files in [true, false] {
            let staged = StagedRevert {
                message_id: "msg_18db09094001",
                files,
            };
            assert_eq!(
                serde_json::to_value(&staged).unwrap_or_default(),
                json!({"messageID": "msg_18db09094001", "files": files})
            );
        }
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
