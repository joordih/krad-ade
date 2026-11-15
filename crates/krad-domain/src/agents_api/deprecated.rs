use serde::{Deserialize, Serialize};

use super::protocol::{ApiError, ApiVersion, ErrorCode, Message, Response};

pub const MAILBOX_REMOVED_IN: ApiVersion = ApiVersion(2);
pub const CHECK_DEPRECATED: &str = "`krad check` is deprecated and will be removed in the next \
                                    agents API version: agents no longer message each other; a \
                                    lead follows its workers with wait and read";
const NOTICE_ID: u64 = 0;
const NOTICE_SENDER: u64 = 0;
const NOTICE_TITLE: &str = "krad";
const ONLY_LEADS_SEND: &str = "Only the session's lead can send to its workers: agents no longer \
                               message each other; a lead starts workers with spawn and follows \
                               them with wait and read";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Deprecation {
    pub removed_in: ApiVersion,
}

#[must_use]
pub fn check_answer() -> Response {
    Response::Messages(vec![Message {
        id: NOTICE_ID,
        from: NOTICE_SENDER,
        from_title: NOTICE_TITLE.to_owned(),
        body: CHECK_DEPRECATED.to_owned(),
        deprecated: Some(Deprecation {
            removed_in: MAILBOX_REMOVED_IN,
        }),
    }])
}

#[must_use]
pub fn only_leads_send() -> Response {
    Response::Error(ApiError::new(ErrorCode::Forbidden, ONLY_LEADS_SEND))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents_api::{
        lead_refusal, Caller, LeadHandle, Request, TerminalHandle, API_VERSION,
    };

    #[test]
    fn check_answers_no_mail_but_a_notice_old_clients_print_and_new_ones_recognise() {
        let notices: Vec<Message> = match check_answer() {
            Response::Messages(messages) => messages,
            _ => Vec::new(),
        };
        let [notice] = notices.as_slice() else {
            unreachable!("check answers one notice, got {notices:?}");
        };
        assert_eq!(notice.body, CHECK_DEPRECATED);
        assert!(MAILBOX_REMOVED_IN > API_VERSION);
        assert_eq!(
            notice.deprecated.map(|deprecation| deprecation.removed_in),
            Some(MAILBOX_REMOVED_IN)
        );
    }

    #[test]
    fn a_message_written_by_an_older_app_reads_as_not_deprecated() -> serde_json::Result<()> {
        let message: Message = serde_json::from_str(
            r#"{"id":3,"from":4,"from_title":"Claude Code · 4","body":"API done"}"#,
        )?;
        assert_eq!(message.deprecated, None);
        assert!(!serde_json::to_string(&message)?.contains("deprecated"));
        Ok(())
    }

    #[test]
    fn a_send_from_anyone_but_a_lead_is_refused_towards_leads_and_workers() {
        let send = Request::Send {
            to: 12,
            body: "API done".to_owned(),
        };
        let peer = Caller::Terminal(TerminalHandle {
            environment: "main".to_owned(),
            terminal: 4,
        });
        assert_eq!(lead_refusal(&peer, &send), Some(only_leads_send()));
        let lead = Caller::Lead(LeadHandle {
            environment: "main".to_owned(),
            session: 1,
        });
        assert_eq!(lead_refusal(&lead, &send), None);
    }
}
