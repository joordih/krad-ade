use crate::client::REQUEST_TIMEOUT;
use crate::config::GitlabConfig;
use crate::encode::{form_decode, percent_encode};
use crate::pkce::{random_secret, Pkce};
use krad_domain::GitlabTokens;
use serde::Deserialize;
use std::fmt;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::{Duration, Instant};
use ureq::Agent;

const AUTHORIZE_PATH: &str = "/oauth/authorize";
const TOKEN_PATH: &str = "/oauth/token";
const DEVICE_PATH: &str = "/oauth/authorize_device";
macro_rules! loopback_origin {
    () => {
        "http://127.0.0.1"
    };
}
macro_rules! callback_path {
    () => {
        "/callback"
    };
}
const CALLBACK_PATH: &str = callback_path!();
const LOOPBACK_ORIGIN: &str = loopback_origin!();
pub const SCOPE: &str = "api";
pub const REDIRECT_URI: &str = concat!(loopback_origin!(), callback_path!());
const CHALLENGE_METHOD: &str = "S256";
const RESPONSE_TYPE: &str = "code";
const AUTHORIZATION_CODE_GRANT: &str = "authorization_code";
const REFRESH_GRANT: &str = "refresh_token";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const AUTHORIZATION_PENDING: &str = "authorization_pending";
const SLOW_DOWN: &str = "slow_down";
const EXPIRED_TOKEN: &str = "expired_token";
const ACCESS_DENIED: &str = "access_denied";
const INVALID_GRANT: &str = "invalid_grant";
const CODE_PARAM: &str = "code";
const GRANT_TYPE_PARAM: &str = "grant_type";
const REFRESH_TOKEN_PARAM: &str = "refresh_token";
const CLIENT_ID_PARAM: &str = "client_id";
const REDIRECT_URI_PARAM: &str = "redirect_uri";
const CODE_VERIFIER_PARAM: &str = "code_verifier";
const DEVICE_CODE_PARAM: &str = "device_code";
const SCOPE_PARAM: &str = "scope";
const RESPONSE_TYPE_PARAM: &str = "response_type";
const CODE_CHALLENGE_PARAM: &str = "code_challenge";
const CHALLENGE_METHOD_PARAM: &str = "code_challenge_method";
const STATE_PARAM: &str = "state";
const ERROR_PARAM: &str = "error";
const ERROR_DESCRIPTION_PARAM: &str = "error_description";
const QUERY_START: char = '?';
const PAIR_SEPARATOR: &str = "&";
const KEY_SEPARATOR: char = '=';
const REQUEST_METHOD: &str = "GET";
const LOGIN_TIMEOUT: Duration = Duration::from_mins(5);
const ACCEPT_POLL: Duration = Duration::from_millis(200);
const CALLBACK_READ_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REQUEST_LINE: u64 = 8 * 1024;
const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);
const SIGNED_IN_PAGE: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nSigned in to GitLab. You can close this tab and return to Krad ADE.\r\n";
const FAILED_PAGE: &str = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nThe GitLab sign-in did not finish. Return to Krad ADE and try again.\r\n";
const NOT_FOUND_PAGE: &str = "HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n";

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("Could not wait for the GitLab sign-in: {0}")]
    Callback(#[from] std::io::Error),
    #[error("Stopped waiting for the GitLab sign-in")]
    TimedOut,
    #[error("The GitLab sign-in answered for another request; try again")]
    StateMismatch,
    #[error("GitLab refused the sign-in: {0}")]
    Denied(String),
    #[error("The GitLab login expired; sign in again")]
    Expired,
    #[error("GitLab sign-in request failed: {0}")]
    Http(#[from] ureq::Error),
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
}

impl From<TokenResponse> for GitlabTokens {
    fn from(response: TokenResponse) -> Self {
        Self {
            access_token: response.access_token,
            refresh_token: response.refresh_token,
        }
    }
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

impl ErrorResponse {
    fn into_error(self) -> OAuthError {
        match self.error.as_str() {
            EXPIRED_TOKEN | INVALID_GRANT => OAuthError::Expired,
            _ => OAuthError::Denied(self.error_description.unwrap_or(self.error)),
        }
    }
}

enum TokenAnswer {
    Granted(GitlabTokens),
    Refused(ErrorResponse),
}

pub(crate) fn oauth_agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .new_agent()
}

fn request_tokens(
    agent: &Agent,
    config: &GitlabConfig,
    form: &[(&str, &str)],
) -> Result<TokenAnswer, OAuthError> {
    let mut response = agent
        .post(&config.oauth_url(TOKEN_PATH))
        .send_form(form.iter().copied())?;
    if response.status().is_success() {
        let tokens = response.body_mut().read_json::<TokenResponse>()?;
        return Ok(TokenAnswer::Granted(tokens.into()));
    }
    Ok(TokenAnswer::Refused(
        response.body_mut().read_json::<ErrorResponse>()?,
    ))
}

fn granted(answer: TokenAnswer) -> Result<GitlabTokens, OAuthError> {
    match answer {
        TokenAnswer::Granted(tokens) => Ok(tokens),
        TokenAnswer::Refused(refusal) => Err(refusal.into_error()),
    }
}

pub(crate) fn refresh(
    agent: &Agent,
    config: &GitlabConfig,
    tokens: &GitlabTokens,
) -> Result<GitlabTokens, OAuthError> {
    granted(request_tokens(
        agent,
        config,
        &[
            (GRANT_TYPE_PARAM, REFRESH_GRANT),
            (REFRESH_TOKEN_PARAM, &tokens.refresh_token),
            (CLIENT_ID_PARAM, &config.client_id),
        ],
    )?)
}

pub struct LoopbackLogin {
    config: GitlabConfig,
    listener: TcpListener,
    redirect_uri: String,
    pkce: Pkce,
    state: String,
}

impl LoopbackLogin {
    /// # Errors
    ///
    /// [`OAuthError::Callback`] when no port can be opened.
    pub fn start(config: GitlabConfig) -> Result<Self, OAuthError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            config,
            listener,
            redirect_uri: format!("{LOOPBACK_ORIGIN}:{port}{CALLBACK_PATH}"),
            pkce: Pkce::new(),
            state: random_secret(),
        })
    }

    #[must_use]
    pub fn authorize_url(&self) -> String {
        let params = [
            (CLIENT_ID_PARAM, self.config.client_id.as_str()),
            (REDIRECT_URI_PARAM, &self.redirect_uri),
            (RESPONSE_TYPE_PARAM, RESPONSE_TYPE),
            (STATE_PARAM, &self.state),
            (SCOPE_PARAM, SCOPE),
            (CODE_CHALLENGE_PARAM, &self.pkce.challenge),
            (CHALLENGE_METHOD_PARAM, CHALLENGE_METHOD),
        ];
        let query: Vec<String> = params
            .iter()
            .map(|(key, value)| format!("{key}={}", percent_encode(value)))
            .collect();
        format!(
            "{}{QUERY_START}{}",
            self.config.oauth_url(AUTHORIZE_PATH),
            query.join(PAIR_SEPARATOR)
        )
    }

    /// # Errors
    ///
    /// [`OAuthError::TimedOut`] when nobody signs in in time,
    /// [`OAuthError::Denied`] when the user refuses, and the other variants
    /// when the redirect or the exchange fails.
    pub fn finish(self) -> Result<GitlabTokens, OAuthError> {
        let code = self.await_code(LOGIN_TIMEOUT)?;
        granted(request_tokens(
            &oauth_agent(),
            &self.config,
            &[
                (GRANT_TYPE_PARAM, AUTHORIZATION_CODE_GRANT),
                (CODE_PARAM, &code),
                (REDIRECT_URI_PARAM, &self.redirect_uri),
                (CLIENT_ID_PARAM, &self.config.client_id),
                (CODE_VERIFIER_PARAM, &self.pkce.verifier),
            ],
        )?)
    }

    fn await_code(&self, timeout: Duration) -> Result<String, OAuthError> {
        self.listener.set_nonblocking(true)?;
        let deadline = Instant::now() + timeout;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(outcome) = self.answer(stream)? {
                        return outcome;
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(OAuthError::TimedOut);
                    }
                    std::thread::sleep(ACCEPT_POLL);
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn answer(&self, mut stream: TcpStream) -> std::io::Result<Option<Result<String, OAuthError>>> {
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(CALLBACK_READ_TIMEOUT))?;
        let mut line = String::new();
        BufReader::new((&stream).take(MAX_REQUEST_LINE)).read_line(&mut line)?;
        let outcome = parse_callback(&line, &self.state);
        let page = match &outcome {
            None => NOT_FOUND_PAGE,
            Some(Ok(_)) => SIGNED_IN_PAGE,
            Some(Err(_)) => FAILED_PAGE,
        };
        stream.write_all(page.as_bytes())?;
        Ok(outcome)
    }
}

fn parse_callback(request_line: &str, state: &str) -> Option<Result<String, OAuthError>> {
    let mut parts = request_line.split_whitespace();
    if parts.next() != Some(REQUEST_METHOD) {
        return None;
    }
    let (path, query) = parts.next()?.split_once(QUERY_START)?;
    if path != CALLBACK_PATH {
        return None;
    }
    let param = |name: &str| {
        query
            .split(PAIR_SEPARATOR)
            .filter_map(|pair| pair.split_once(KEY_SEPARATOR))
            .find(|(key, _)| *key == name)
            .and_then(|(_, value)| form_decode(value))
    };
    if param(STATE_PARAM).as_deref() != Some(state) {
        return Some(Err(OAuthError::StateMismatch));
    }
    if let Some(error) = param(ERROR_PARAM) {
        return Some(Err(OAuthError::Denied(
            param(ERROR_DESCRIPTION_PARAM).unwrap_or(error),
        )));
    }
    Some(param(CODE_PARAM).ok_or_else(|| OAuthError::Denied(ACCESS_DENIED.to_owned())))
}

#[derive(Deserialize)]
struct DeviceResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: u64,
}

pub struct DeviceLogin {
    config: GitlabConfig,
    device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
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

impl DeviceLogin {
    /// # Errors
    ///
    /// [`OAuthError::Callback`] when no port can be opened.
    pub fn start(config: GitlabConfig) -> Result<Self, OAuthError> {
        let mut response = oauth_agent()
            .post(&config.oauth_url(DEVICE_PATH))
            .send_form([
                (CLIENT_ID_PARAM, config.client_id.as_str()),
                (SCOPE_PARAM, SCOPE),
            ])?;
        if !response.status().is_success() {
            return Err(response
                .body_mut()
                .read_json::<ErrorResponse>()?
                .into_error());
        }
        let device = response.body_mut().read_json::<DeviceResponse>()?;
        Ok(Self::new(config, device))
    }

    fn new(config: GitlabConfig, device: DeviceResponse) -> Self {
        Self {
            config,
            device_code: device.device_code,
            user_code: device.user_code,
            verification_uri: device.verification_uri,
            verification_uri_complete: device.verification_uri_complete,
            expires_in: Duration::from_secs(device.expires_in),
            interval: Duration::from_secs(device.interval),
        }
    }

    /// # Errors
    ///
    /// [`OAuthError::TimedOut`] when the code expires unused,
    /// [`OAuthError::Denied`] when the user refuses, [`OAuthError::Http`]
    /// when GitLab cannot be reached.
    pub fn finish(&self) -> Result<GitlabTokens, OAuthError> {
        let agent = oauth_agent();
        let deadline = Instant::now() + self.expires_in;
        let mut interval = self.interval;
        loop {
            std::thread::sleep(interval);
            let answer = request_tokens(
                &agent,
                &self.config,
                &[
                    (GRANT_TYPE_PARAM, DEVICE_GRANT),
                    (DEVICE_CODE_PARAM, &self.device_code),
                    (CLIENT_ID_PARAM, &self.config.client_id),
                ],
            )?;
            match answer {
                TokenAnswer::Granted(tokens) => return Ok(tokens),
                TokenAnswer::Refused(refusal) => match refusal.error.as_str() {
                    AUTHORIZATION_PENDING => {}
                    SLOW_DOWN => interval += SLOW_DOWN_STEP,
                    EXPIRED_TOKEN => return Err(OAuthError::TimedOut),
                    _ => return Err(refusal.into_error()),
                },
            }
            if Instant::now() >= deadline {
                return Err(OAuthError::TimedOut);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = include_str!("../fixtures/token.json");
    const DEVICE: &str = include_str!("../fixtures/device.json");

    fn config() -> GitlabConfig {
        GitlabConfig {
            host: "https://gitlab.example.test".to_owned(),
            client_id: "app id".to_owned(),
        }
    }

    fn query_param(url: &str, name: &str) -> Option<String> {
        url.split_once(QUERY_START)?
            .1
            .split(PAIR_SEPARATOR)
            .filter_map(|pair| pair.split_once(KEY_SEPARATOR))
            .find(|(key, _)| *key == name)
            .and_then(|(_, value)| form_decode(value))
    }

    #[test]
    fn the_authorize_url_carries_pkce_state_and_the_loopback_redirect() -> Result<(), OAuthError> {
        let login = LoopbackLogin::start(config())?;
        let url = login.authorize_url();
        assert!(url.starts_with("https://gitlab.example.test/oauth/authorize?"));
        assert_eq!(
            query_param(&url, CLIENT_ID_PARAM).as_deref(),
            Some("app id")
        );
        assert_eq!(query_param(&url, SCOPE_PARAM).as_deref(), Some(SCOPE));
        assert_eq!(
            query_param(&url, CHALLENGE_METHOD_PARAM).as_deref(),
            Some(CHALLENGE_METHOD)
        );
        assert_eq!(
            query_param(&url, CODE_CHALLENGE_PARAM),
            Some(login.pkce.challenge.clone())
        );
        assert_eq!(query_param(&url, STATE_PARAM), Some(login.state.clone()));
        let redirect = query_param(&url, REDIRECT_URI_PARAM).unwrap_or_default();
        assert!(redirect.starts_with("http://127.0.0.1:"));
        assert!(redirect.ends_with("/callback"));
        assert_eq!(REDIRECT_URI, "http://127.0.0.1/callback");
        assert!(!url.contains(&login.pkce.verifier));
        Ok(())
    }

    #[test]
    fn the_redirect_yields_its_code_only_with_the_right_state() {
        let line = "GET /callback?code=a%2Fb&state=s1 HTTP/1.1\r\n";
        assert!(matches!(parse_callback(line, "s1"), Some(Ok(code)) if code == "a/b"));
        assert!(matches!(
            parse_callback(line, "s2"),
            Some(Err(OAuthError::StateMismatch))
        ));
        assert!(parse_callback("GET /favicon.ico HTTP/1.1\r\n", "s1").is_none());
        assert!(matches!(
            parse_callback(
                "GET /callback?error=access_denied&error_description=No+thanks&state=s1 HTTP/1.1",
                "s1"
            ),
            Some(Err(OAuthError::Denied(text))) if text == "No thanks"
        ));
    }

    #[test]
    fn the_listener_skips_other_requests_and_returns_the_code() -> Result<(), OAuthError> {
        let login = LoopbackLogin::start(config())?;
        let address = login.listener.local_addr()?;
        let state = login.state.clone();
        let browser = std::thread::spawn(move || -> std::io::Result<String> {
            let mut favicon = TcpStream::connect(address)?;
            favicon.write_all(b"GET /favicon.ico HTTP/1.1\r\n\r\n")?;
            let mut ignored = String::new();
            favicon.read_to_string(&mut ignored)?;
            let mut redirect = TcpStream::connect(address)?;
            redirect.write_all(
                format!("GET /callback?code=c0de&state={state} HTTP/1.1\r\n\r\n").as_bytes(),
            )?;
            let mut page = String::new();
            redirect.read_to_string(&mut page)?;
            Ok(page)
        });
        assert_eq!(login.await_code(Duration::from_secs(10))?, "c0de");
        let page = browser.join().map_err(|_| OAuthError::TimedOut)??;
        assert!(page.starts_with("HTTP/1.1 200"));
        Ok(())
    }

    #[test]
    fn token_and_device_answers_parse() -> serde_json::Result<()> {
        let tokens: GitlabTokens = serde_json::from_str::<TokenResponse>(TOKEN)?.into();
        assert_eq!(tokens.refresh_token, "fixture-refresh");
        let device = DeviceLogin::new(config(), serde_json::from_str(DEVICE)?);
        assert_eq!(device.user_code, "ABCD-EFGH");
        assert_eq!(device.interval, Duration::from_secs(5));
        assert!(!format!("{device:?}").contains("fixture-device-code"));
        Ok(())
    }

    #[test]
    fn an_invalid_grant_means_signing_in_again() -> serde_json::Result<()> {
        let refusal: ErrorResponse = serde_json::from_str(r#"{"error":"invalid_grant"}"#)?;
        assert!(matches!(refusal.into_error(), OAuthError::Expired));
        Ok(())
    }
}
