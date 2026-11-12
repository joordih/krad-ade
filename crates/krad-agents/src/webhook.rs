pub(crate) mod signature;

use krad_domain::{
    DeliveryOutcome, EventRecord, Timestamp, Webhook, WebhookSecret, WebhookSigning,
};

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
}

impl From<&WebhookPostError> for DeliveryOutcome {
    fn from(_: &WebhookPostError) -> Self {
        Self::Refused
    }
}

pub struct WebhookRequest {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

/// # Errors
///
/// Fails when the record cannot be written or a signed hook has no secret.
pub fn webhook_request(
    hook: &Webhook,
    record: &EventRecord,
    secret: Option<&WebhookSecret>,
) -> Result<WebhookRequest, WebhookPostError> {
    let body = String::try_from(record)?;
    let sent_at = Timestamp::now().to_string();
    let signature = match hook.signing {
        WebhookSigning::Signed => {
            let secret = secret.ok_or(WebhookPostError::NoSecret)?;
            Some(signature_header(secret, &sent_at, body.as_bytes()))
        }
        WebhookSigning::Unsigned => None,
    };
    let mut headers = vec![
        (CONTENT_TYPE_HEADER, JSON_MEDIA.to_owned()),
        (USER_AGENT_HEADER, USER_AGENT.to_owned()),
        (TIMESTAMP_HEADER, sent_at),
    ];
    headers.extend(signature.map(|signature| (SIGNATURE_HEADER, signature)));
    Ok(WebhookRequest {
        url: hook.url.as_str().to_owned(),
        headers,
        body,
    })
}
