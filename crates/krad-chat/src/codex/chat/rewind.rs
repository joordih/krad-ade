use krad_domain::RewindPoint;
use serde::Serialize;
use serde_json::Value;

use super::{AppServer, Worker};
use crate::control::LiveEvent;
use crate::rewind::{RewindError, RewindRequest, Rewound};

const THREAD_REVERT: &str = "thread/revert";

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadRevert<'a> {
    thread_id: &'a str,
    before_turn_id: &'a str,
}

fn thread_revert<'a>(
    thread: &'a str,
    request: &'a RewindRequest,
) -> Result<ThreadRevert<'a>, RewindError> {
    let RewindPoint::Turn(turn) = &request.point else {
        return Err(RewindError::UnknownPoint);
    };
    if request.scope.restores_files() {
        return Err(RewindError::KeepsFiles);
    }
    Ok(ThreadRevert {
        thread_id: thread,
        before_turn_id: turn.as_str(),
    })
}

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn rewind(
        &self,
        server: &AppServer,
        request: &RewindRequest,
    ) -> Result<Rewound, RewindError> {
        let revert = thread_revert(&self.thread, request)?;
        if self.translator.turn.is_some() {
            return Err(RewindError::Busy);
        }
        server
            .call::<_, Value>(THREAD_REVERT, &revert)
            .map_err(|error| RewindError::Refused(error.to_string()))?;
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
    fn a_revert_drops_the_edited_turn_and_every_later_one() {
        let edited = request(
            RewindPoint::Turn(RewindId::from("turn_2")),
            RewindScope::Conversation,
        );
        assert_eq!(
            thread_revert("thr_1", &edited).map(|revert| serde_json::to_value(&revert).ok()),
            Ok(Some(json!({"threadId": "thr_1", "beforeTurnId": "turn_2"})))
        );
    }

    #[test]
    fn a_revert_cannot_bring_back_files_or_cut_at_a_message() {
        let with_files = request(
            RewindPoint::Turn(RewindId::from("turn_2")),
            RewindScope::ConversationAndFiles,
        );
        assert_eq!(
            thread_revert("thr_1", &with_files),
            Err(RewindError::KeepsFiles)
        );
        let message = request(
            RewindPoint::Message(RewindId::from("msg_1")),
            RewindScope::Conversation,
        );
        assert_eq!(
            thread_revert("thr_1", &message),
            Err(RewindError::UnknownPoint)
        );
    }
}
