use krad_domain::GithubToken;
use serde::Deserialize;
use std::fmt;
use std::time::{Duration, Instant};
use ureq::Agent;

const CLIENT_ID: Option<&str> = option_env!("KRAD_GITHUB_CLIENT_ID");
pub const SCOPE: &str = "repo";
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const CLIENT_ID_PARAM: &str = "client_id";
const SCOPE_PARAM: &str = "scope";
const DEVICE_CODE_PARAM: &str = "device_code";
const GRANT_TYPE_PARAM: &str = "grant_type";
const ACCEPT: &str = "Accept";
const JSON: &str = "application/json";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);
const PENDING: &str = "authorization_pending";
const SLOW_DOWN: &str = "slow_down";
const EXPIRED: &str = "expired_token";

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("This build of krad-ade has no GitHub app to sign in with")]
    MissingClientId,
    #[error("The GitHub code expired before it was entered; try again")]
    TimedOut,
    #[error("GitHub refused the sign-in: {0}")]
    Denied(String),
    #[error("GitHub sign-in request failed: {0}")]
    Http(#[from] ureq::Error),
}

#[derive(Deserialize)]
struct DeviceResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TokenAnswer {
    Token { access_token: String },
    Refusal { error: String },
}

pub struct DeviceLogin {
    client_id: String,
    device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: Duration,
    interval: Duration,
}

impl fmt::Debug for DeviceLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceLogin")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .finish_non_exhaustive()
    }
}

fn agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .new_agent()
}

impl DeviceLogin {
    /// # Errors
    ///
    /// [`DeviceError::MissingClientId`] when the build carries no client
    /// ID, [`DeviceError::Denied`] when GitHub refuses it, and
    /// [`DeviceError::Http`] when it cannot be reached.
    pub fn start() -> Result<Self, DeviceError> {
        let client_id = CLIENT_ID.ok_or(DeviceError::MissingClientId)?;
        let text = agent()
            .post(DEVICE_CODE_URL)
            .header(ACCEPT, JSON)
            .send_form([(CLIENT_ID_PARAM, client_id), (SCOPE_PARAM, SCOPE)])?
            .body_mut()
            .read_to_string()?;
        let device = serde_json::from_str::<DeviceResponse>(&text)
            .map_err(|_| DeviceError::Denied(refusal_of(&text)))?;
        Ok(Self::new(client_id, device))
    }

    fn new(client_id: &str, device: DeviceResponse) -> Self {
        Self {
            client_id: client_id.to_owned(),
            device_code: device.device_code,
            user_code: device.user_code,
            verification_uri: device.verification_uri,
            expires_in: Duration::from_secs(device.expires_in),
            interval: Duration::from_secs(device.interval),
        }
    }

    /// # Errors
    ///
    /// [`DeviceError::TimedOut`] when the code expires unused,
    /// [`DeviceError::Denied`] when the user refuses, [`DeviceError::Http`]
    /// when GitHub cannot be reached.
    pub fn finish(&self) -> Result<GithubToken, DeviceError> {
        let agent = agent();
        let deadline = Instant::now() + self.expires_in;
        let mut interval = self.interval;
        loop {
            std::thread::sleep(interval);
            if Instant::now() >= deadline {
                return Err(DeviceError::TimedOut);
            }
            let answer = agent
                .post(TOKEN_URL)
                .header(ACCEPT, JSON)
                .send_form([
                    (CLIENT_ID_PARAM, self.client_id.as_str()),
                    (DEVICE_CODE_PARAM, self.device_code.as_str()),
                    (GRANT_TYPE_PARAM, DEVICE_GRANT),
                ])?
                .body_mut()
                .read_json::<TokenAnswer>()?;
            match answer {
                TokenAnswer::Token { access_token } => return Ok(GithubToken { access_token }),
                TokenAnswer::Refusal { error } => match error.as_str() {
                    PENDING => {}
                    SLOW_DOWN => interval += SLOW_DOWN_STEP,
                    EXPIRED => return Err(DeviceError::TimedOut),
                    _ => return Err(DeviceError::Denied(error)),
                },
            }
        }
    }
}

fn refusal_of(text: &str) -> String {
    #[derive(Deserialize)]
    struct Refusal {
        error: String,
    }
    serde_json::from_str::<Refusal>(text).map_or_else(|_| text.trim().to_owned(), |r| r.error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_answer_parses_and_hides_its_device_code() -> serde_json::Result<()> {
        let device: DeviceResponse = serde_json::from_str(
            r#"{"device_code":"secret-device","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#,
        )?;
        let login = DeviceLogin::new("app", device);
        assert_eq!(login.user_code, "WDJB-MJHT");
        assert_eq!(login.interval, Duration::from_secs(5));
        assert!(!format!("{login:?}").contains("secret-device"));
        Ok(())
    }

    #[test]
    fn a_poll_answers_with_a_token_or_a_reason() -> serde_json::Result<()> {
        let token: TokenAnswer = serde_json::from_str(
            r#"{"access_token":"gho_x","token_type":"bearer","scope":"repo"}"#,
        )?;
        assert!(matches!(token, TokenAnswer::Token { access_token } if access_token == "gho_x"));
        let pending: TokenAnswer = serde_json::from_str(
            r#"{"error":"authorization_pending","error_description":"wait"}"#,
        )?;
        assert!(matches!(pending, TokenAnswer::Refusal { error } if error == PENDING));
        assert_eq!(
            refusal_of(r#"{"error":"device_flow_disabled"}"#),
            "device_flow_disabled"
        );
        Ok(())
    }
}
