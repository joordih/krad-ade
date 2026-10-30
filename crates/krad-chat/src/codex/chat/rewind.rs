use krad_domain::RewindPoint;
use serde::Serialize;
use serde_json::Value;

use super::{AppServer, Worker};
use crate::control::LiveEvent;
use crate::rewind::{RewindError, RewindRequest, Rewound};

const THREAD_REVERT: &str = "thread/revert";

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadRevert<'a> {
    pub(super) thread_id: &'a str,
    pub(super) before_turn_id: &'a str,
}

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn rewind(
        &self,
        server: &AppServer,
        request: &RewindRequest,
    ) -> Result<Rewound, RewindError> {
        let RewindPoint::Turn(turn) = &request.point else {
            return Err(RewindError::UnknownPoint);
        };
        if request.scope.restores_files() {
            return Err(RewindError::KeepsFiles);
        }
        if self.translator.turn.is_some() {
            return Err(RewindError::Busy);
        }
        server
            .call::<_, Value>(
                THREAD_REVERT,
                &ThreadRevert {
                    thread_id: &self.thread,
                    before_turn_id: turn.as_str(),
                },
            )
            .map_err(|error| RewindError::Refused(error.to_string()))?;
        Ok(Rewound::InPlace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_revert_drops_the_edited_turn_and_every_later_one() {
        let revert = ThreadRevert {
            thread_id: "thr_1",
            before_turn_id: "turn_2",
        };
        assert_eq!(
            serde_json::to_value(&revert).unwrap_or_default(),
            json!({"threadId": "thr_1", "beforeTurnId": "turn_2"})
        );
    }
}
