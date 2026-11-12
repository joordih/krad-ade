mod signature;

use krad_domain::{DeliveryOutcome, EventRecord, Timestamp, Webhook, WebhookSecret, WebhookSigning};
use std::time::Duration;
use ureq::Agent;

use signature::signature_header;

const CONTENT_TYPE_HEADER: &str = "content-type";
const JSON_MEDIA: &str = "application/json";
const USER_AGENT_HEADER: &str = "user-agent";
const USER_AGENT: &str = "krad-ade-webhooks";
const SIGNATURE_HEADER: &str = "x-krad-signature-256";
const TIMESTAMP_HEADER: &str = "x-krad-timestamp";

#[derive(Debug, thiserror::Error)]
pub enum WebhookPostError {
    #[error("the event could not be written: {0}")]
    Body(#[from] serde_json::Error),
    #[error("a signed webhook has no secret")]
    NoSecret,
    #[error("the webhook was not reached: {0}")]
    Unreached(#[from] ureq::Error),
}

impl From<&WebhookPostError> for DeliveryOutcome {
    fn from(error: &WebhookPostError) -> Self {
        match error {
            WebhookPostError::Body(_) | WebhookPostError::NoSecret => Self::Refused,
            WebhookPostError::Unreached(_) => Self::Failed,
        }
    }
}

#[must_use]
pub fn webhook_agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

/// # Errors
///
/// Fails when the record cannot be written, a signed hook has no secret or the hook is not
/// reached; an answer with any status is an outcome, not an error.
pub fn post_webhook(
    agent: &Agent,
    hook: &Webhook,
    record: &EventRecord,
    secret: Option<&WebhookSecret>,
) -> Result<DeliveryOutcome, WebhookPostError> {
    let body = String::try_from(record)?;
    let sent_at = Timestamp::now().to_string();
    let mut request = agent
        .post(hook.url.as_str())
        .header(CONTENT_TYPE_HEADER, JSON_MEDIA)
        .header(USER_AGENT_HEADER, USER_AGENT)
        .header(TIMESTAMP_HEADER, &sent_at);
    if hook.signing == WebhookSigning::Signed {
        let secret = secret.ok_or(WebhookPostError::NoSecret)?;
        let signature = signature_header(secret, &sent_at, body.as_bytes());
        request = request.header(SIGNATURE_HEADER, signature);
    }
    let response = request.send(body.as_bytes())?;
    Ok(DeliveryOutcome::of_status(response.status().as_u16()))
}
