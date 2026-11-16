use krad_domain::{DeviceName, InstallId, Pairing, PairingCode};
use krad_domain::{EnvironmentName, PairingPermission};
use krad_protocol::{Answer, Command, EventKind, Feed, FileAsk, FileCommand, Reply, Scope};
use krad_runner::{RedeemRefusal, RuntimeServer};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::Duration;
use tungstenite::handshake::derive_accept_key;
use tungstenite::protocol::Role;
use tungstenite::WebSocket;

use crate::assets::asset;
use crate::cookie::{clear_pairing, pairing_of, set_pairing};
use crate::guard::Guard;
use crate::http::{read_request, HttpError, Request, Response, STRICT_TRANSPORT, UPLOAD_PATH};
use crate::push::{Push, Subscription};
use crate::socket::{self, WebStream};
use crate::tls::Wire;

pub(crate) const RUNTIME_PATH: &str = "/runtime";
pub(crate) const PAIRING_PATH: &str = "/pairing";
pub(crate) const THEME_PATH: &str = "/theme.css";
pub(crate) const PUSH_PATH: &str = "/push";
const CSS: &str = "text/css; charset=utf-8";
const HEAD_TIMEOUT: Duration = Duration::from_secs(15);
const JSON: &str = "application/json";

const OK: (u16, &str) = (200, "OK");
const NO_CONTENT: (u16, &str) = (204, "No Content");
const BAD_REQUEST: (u16, &str) = (400, "Bad Request");
const FORBIDDEN: (u16, &str) = (403, "Forbidden");
const NOT_FOUND: (u16, &str) = (404, "Not Found");
const METHOD_NOT_ALLOWED: (u16, &str) = (405, "Method Not Allowed");
const TOO_LARGE: (u16, &str) = (413, "Content Too Large");
const TOO_MANY: (u16, &str) = (429, "Too Many Requests");
const FAILED: (u16, &str) = (500, "Internal Server Error");
const UNAVAILABLE: (u16, &str) = (503, "Service Unavailable");
const SWITCHING: &str = "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ";

#[derive(Deserialize)]
struct Redeem {
    code: PairingCode,
    install: InstallId,
    device: DeviceName,
}

#[derive(Deserialize)]
struct Lapsed {
    endpoint: String,
}

#[derive(Serialize)]
struct PushKey {
    key: String,
}

#[derive(Serialize)]
struct Refusal<'a> {
    error: &'a str,
}

pub(crate) struct Routes {
    pub(crate) server: Arc<RuntimeServer>,
    pub(crate) guard: Guard,
    pub(crate) secure: bool,
    pub(crate) streams: SyncSender<WebStream>,
    pub(crate) push: Option<Arc<Push>>,
}

enum Next {
    Keep,
    Close,
    Upgrade(Option<Pairing>),
}

fn status((status, reason): (u16, &'static str)) -> Response<'static> {
    Response::new(status, reason)
}

impl Routes {
    pub(crate) fn serve(&self, mut wire: Wire) {
        if let Err(error) = wire.set_read_timeout(HEAD_TIMEOUT) {
            tracing::debug!(%error, "a web connection could not be timed");
            return;
        }
        let mut pending = Vec::new();
        loop {
            let request = match read_request(&mut wire, &mut pending) {
                Ok(Some(request)) => request,
                Ok(None) => return,
                Err(HttpError::BodyTooLarge) => {
                    self.answer(&mut wire, &status(TOO_LARGE));
                    return;
                }
                Err(error) => {
                    tracing::debug!(%error, "a web request could not be read");
                    return;
                }
            };
            let closes = request.closes();
            match self.route(&mut wire, &request) {
                Next::Upgrade(pairing) => return self.hand_over(wire, pending, pairing),
                Next::Close => return,
                Next::Keep if closes => return,
                Next::Keep => {}
            }
        }
    }

    fn hand_over(&self, wire: Wire, pending: Vec<u8>, pairing: Option<Pairing>) {
        let socket =
            WebSocket::from_partially_read(wire, pending, Role::Server, Some(socket::config()));
        match socket::start(socket, pairing) {
            Ok(stream) => {
                if self.streams.send(stream).is_err() {
                    tracing::debug!("the runtime stopped taking web connections");
                }
            }
            Err(error) => tracing::debug!(%error, "a web socket could not start"),
        }
    }

    fn answer(&self, wire: &mut Wire, response: &Response<'_>) -> Next {
        match response.write(wire, self.secure) {
            Ok(()) => Next::Keep,
            Err(error) => {
                tracing::debug!(%error, "a web response could not be written");
                Next::Close
            }
        }
    }

    fn refuse(&self, wire: &mut Wire, code: (u16, &'static str), error: &str) -> Next {
        let body = serde_json::to_vec(&Refusal { error }).unwrap_or_default();
        self.answer(wire, &status(code).with_body(JSON, &body))
    }

    fn route(&self, wire: &mut Wire, request: &Request) -> Next {
        if !self.guard.known_host(request) {
            return self.refuse(wire, FORBIDDEN, "unknown host");
        }
        let path = request.path.split('?').next().unwrap_or_default();
        match (request.method.as_str(), path) {
            ("GET", RUNTIME_PATH) => self.upgrade(wire, request),
            ("POST", PAIRING_PATH) => self.pair(wire, request),
            ("DELETE", PAIRING_PATH) => self.forget(wire, request),
            ("GET", THEME_PATH) => self.theme(wire),
            ("POST", UPLOAD_PATH) => self.upload(wire, request),
            ("GET" | "POST" | "DELETE", PUSH_PATH) => self.push(wire, request),
            ("GET" | "HEAD", _) => match asset(&request.path) {
                Some(found) => {
                    let body = if request.method == "HEAD" {
                        &[][..]
                    } else {
                        found.body
                    };
                    self.answer(wire, &status(OK).with_body(found.content_type, body))
                }
                None => self.answer(wire, &status(NOT_FOUND)),
            },
            _ => self.answer(wire, &status(METHOD_NOT_ALLOWED)),
        }
    }

    fn pair(&self, wire: &mut Wire, request: &Request) -> Next {
        if !self.guard.same_origin(request) {
            return self.refuse(wire, FORBIDDEN, "cross-origin request");
        }
        let Ok(redeem) = serde_json::from_slice::<Redeem>(&request.body) else {
            return self.refuse(
                wire,
                BAD_REQUEST,
                "expected a code, an install and a device",
            );
        };
        match self
            .server
            .redeem(&redeem.code, &redeem.install, redeem.device)
        {
            Ok((token, permissions)) => {
                let pairing = Pairing {
                    install: redeem.install,
                    token,
                };
                let body = serde_json::to_vec(&permissions).unwrap_or_default();
                let response = status(OK)
                    .with_body(JSON, &body)
                    .with_header(set_pairing(&pairing, self.secure));
                self.answer(wire, &response)
            }
            Err(refusal) => {
                let code = match &refusal {
                    RedeemRefusal::UnknownCode => NOT_FOUND,
                    RedeemRefusal::TooManyTries => TOO_MANY,
                    RedeemRefusal::NotKept(error) => {
                        tracing::warn!(%error, "a web pairing could not be kept");
                        FAILED
                    }
                };
                self.refuse(wire, code, &refusal.to_string())
            }
        }
    }

    fn theme(&self, wire: &mut Wire) -> Next {
        let Ok(snapshot) = futures::executor::block_on(self.server.handle.snapshot(Scope::Runtime))
        else {
            return self.answer(wire, &status(UNAVAILABLE));
        };
        let library = krad_palette::theme_library(krad_store::read_user_themes());
        let css = krad_palette::settings_css(&snapshot.settings, &library);
        self.answer(wire, &status(OK).with_body(CSS, css.as_bytes()))
    }

    fn upload(&self, wire: &mut Wire, request: &Request) -> Next {
        if !self.guard.same_origin(request) {
            return self.refuse(wire, FORBIDDEN, "cross-origin request");
        }
        let allowed = pairing_of(request)
            .and_then(|pairing| self.server.authorize(&pairing))
            .is_some_and(|permissions| permissions.allows(PairingPermission::Drive));
        if !allowed {
            return self.refuse(wire, FORBIDDEN, "this browser may not upload files");
        }
        let (Some(environment), Some(dir), Some(name)) = (
            request.query("environment"),
            request.query("dir"),
            request.query("name"),
        ) else {
            return self.refuse(
                wire,
                BAD_REQUEST,
                "expected an environment, a dir and a name",
            );
        };
        let handle = &self.server.handle;
        let mut feed = handle.subscribe();
        let id = handle.next_request();
        let put = Command::File(FileCommand {
            environment: EnvironmentName::new(environment),
            ask: FileAsk::Put {
                dir: PathBuf::from(dir),
                name,
                bytes: request.body.clone(),
            },
        });
        match futures::executor::block_on(handle.send(id, put)) {
            Ok(Reply::Accepted) => {}
            Ok(Reply::Refused { message, .. }) => return self.refuse(wire, BAD_REQUEST, &message),
            Err(_) => return self.answer(wire, &status(UNAVAILABLE)),
        }
        let answer = loop {
            match futures::executor::block_on(futures::StreamExt::next(&mut feed)) {
                Some(Feed::Event(event)) => {
                    if let EventKind::Answered { request, answer } = event.kind {
                        if request == id {
                            break Some(*answer);
                        }
                    }
                }
                Some(Feed::Snapshot(_)) => {}
                None => break None,
            }
        };
        match answer {
            Some(Answer::Placed { path }) => {
                let body =
                    serde_json::to_vec(&serde_json::json!({ "path": path })).unwrap_or_default();
                self.answer(wire, &status(OK).with_body(JSON, &body))
            }
            Some(Answer::Failed { message, .. }) => self.refuse(wire, BAD_REQUEST, &message),
            Some(_) => self.refuse(wire, FAILED, "the runtime answered something else"),
            None => self.answer(wire, &status(UNAVAILABLE)),
        }
    }

    fn push(&self, wire: &mut Wire, request: &Request) -> Next {
        if !self.guard.same_origin(request) {
            return self.refuse(wire, FORBIDDEN, "cross-origin request");
        }
        let allowed = pairing_of(request)
            .and_then(|pairing| self.server.authorize(&pairing))
            .is_some_and(|permissions| permissions.allows(PairingPermission::Read));
        if !allowed {
            return self.refuse(wire, FORBIDDEN, "this browser is not paired");
        }
        let Some(push) = &self.push else {
            return self.answer(wire, &status(UNAVAILABLE));
        };
        let kept = match request.method.as_str() {
            "GET" => {
                let body = serde_json::to_vec(&PushKey {
                    key: push.public_key(),
                })
                .unwrap_or_default();
                return self.answer(wire, &status(OK).with_body(JSON, &body));
            }
            "POST" => match serde_json::from_slice::<Subscription>(&request.body) {
                Ok(subscription) => push.subscribe(subscription),
                Err(_) => return self.refuse(wire, BAD_REQUEST, "expected a push subscription"),
            },
            _ => match serde_json::from_slice::<Lapsed>(&request.body) {
                Ok(lapsed) => push.unsubscribe(&lapsed.endpoint),
                Err(_) => return self.refuse(wire, BAD_REQUEST, "expected an endpoint"),
            },
        };
        match kept {
            Ok(()) => self.answer(wire, &status(NO_CONTENT)),
            Err(error) => self.refuse(wire, BAD_REQUEST, &error.to_string()),
        }
    }

    fn forget(&self, wire: &mut Wire, request: &Request) -> Next {
        if !self.guard.same_origin(request) {
            return self.refuse(wire, FORBIDDEN, "cross-origin request");
        }
        let response = status(NO_CONTENT).with_header(clear_pairing(self.secure));
        self.answer(wire, &response)
    }

    fn upgrade(&self, wire: &mut Wire, request: &Request) -> Next {
        if !self.guard.same_origin(request) {
            return self.refuse(wire, FORBIDDEN, "cross-origin request");
        }
        let key = request.header("sec-websocket-key");
        let upgrading =
            request.has_token("upgrade", "websocket") && request.has_token("connection", "upgrade");
        let (true, Some(key)) = (upgrading, key) else {
            return self.refuse(wire, BAD_REQUEST, "expected a web socket upgrade");
        };
        let transport = if self.secure { STRICT_TRANSPORT } else { "" };
        let answer = format!(
            "{SWITCHING}{}\r\n{transport}\r\n",
            derive_accept_key(key.as_bytes())
        );
        match wire
            .write_all(answer.as_bytes())
            .and_then(|()| wire.flush())
        {
            Ok(()) => Next::Upgrade(pairing_of(request)),
            Err(error) => {
                tracing::debug!(%error, "a web socket upgrade could not be answered");
                Next::Close
            }
        }
    }
}
