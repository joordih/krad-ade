use futures::channel::oneshot;
use krad_chat::{gateway_models, GatewayModel};
use krad_domain::{GatewayAccess, GatewayKey, GatewayUrl};
use krad_store::{delete_gateway_key, find_gateway_key, write_gateway_key, LoginPlace};

use crate::jobs::{JobDone, Jobs};
use crate::runtime::answer_to;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayCheck {
    pub place: LoginPlace,
    pub models: Result<Vec<GatewayModel>, String>,
}

pub type GatewayChecked = Result<Option<GatewayCheck>, String>;

#[derive(Debug)]
pub(crate) enum GatewayInput {
    Check {
        url: Option<GatewayUrl>,
        key: Option<GatewayKey>,
        reply: oneshot::Sender<GatewayChecked>,
    },
    Forget {
        url: GatewayUrl,
        reply: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Debug)]
pub(crate) struct GatewayRead {
    attempt: u64,
    access: Option<GatewayAccess>,
    checked: GatewayChecked,
    reply: oneshot::Sender<GatewayChecked>,
}

#[derive(Debug)]
pub(crate) struct GatewayForgotten {
    removed: Result<(), String>,
    reply: oneshot::Sender<Result<(), String>>,
}

#[derive(Debug, Default)]
pub(crate) struct Gateway {
    attempt: u64,
    pub(crate) access: Option<GatewayAccess>,
}

fn check(url: GatewayUrl, key: Option<&GatewayKey>) -> (Option<GatewayAccess>, GatewayChecked) {
    if let Some(key) = key {
        if let Err(error) = write_gateway_key(&url, key) {
            return (None, Err(error.to_string()));
        }
    }
    let (key, place) = match find_gateway_key(&url) {
        Ok(Some(found)) => found,
        Ok(None) => return (None, Ok(None)),
        Err(error) => return (None, Err(error.to_string())),
    };
    let access = GatewayAccess { url, key };
    let models = gateway_models(&access).map_err(|error| error.to_string());
    let reachable = models.is_ok().then_some(access);
    (reachable, Ok(Some(GatewayCheck { place, models })))
}

impl Gateway {
    pub(crate) fn apply(&mut self, input: GatewayInput, jobs: &Jobs) {
        self.attempt += 1;
        self.access = None;
        match input {
            GatewayInput::Check { url, key, reply } => {
                let Some(url) = url else {
                    return answer_to(reply, Ok(None));
                };
                let attempt = self.attempt;
                let started = jobs.run(move || {
                    let (access, checked) = check(url, key.as_ref());
                    JobDone::GatewayChecked(GatewayRead {
                        attempt,
                        access,
                        checked,
                        reply,
                    })
                });
                if let Err(error) = started {
                    tracing::warn!(%error, "the Gateway check could not start");
                }
            }
            GatewayInput::Forget { url, reply } => {
                let started = jobs.run(move || {
                    let removed = delete_gateway_key(&url).map_err(|error| error.to_string());
                    JobDone::GatewayForgotten(GatewayForgotten { removed, reply })
                });
                if let Err(error) = started {
                    tracing::warn!(%error, "the Gateway key could not be removed");
                }
            }
        }
    }

    pub(crate) fn checked(&mut self, read: GatewayRead) {
        if read.attempt == self.attempt {
            self.access = read.access;
        }
        answer_to(read.reply, read.checked);
    }

    pub(crate) fn forgotten(forgotten: GatewayForgotten) {
        answer_to(forgotten.reply, forgotten.removed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    fn access() -> Option<GatewayAccess> {
        Some(GatewayAccess {
            url: "http://127.0.0.1:1".parse().ok()?,
            key: GatewayKey::parse("sk-test")?,
        })
    }

    #[test]
    fn only_the_latest_check_keeps_its_access_and_every_check_is_answered() {
        let mut gateway = Gateway {
            attempt: 2,
            access: None,
        };
        let (reply, answer) = oneshot::channel();
        gateway.checked(GatewayRead {
            attempt: 1,
            access: access(),
            checked: Ok(None),
            reply,
        });
        assert_eq!(gateway.access, None);
        assert_eq!(block_on(answer).ok(), Some(Ok(None)));
        let (reply, _answer) = oneshot::channel();
        gateway.checked(GatewayRead {
            attempt: 2,
            access: access(),
            checked: Ok(None),
            reply,
        });
        assert_eq!(gateway.access, access());
    }
}
