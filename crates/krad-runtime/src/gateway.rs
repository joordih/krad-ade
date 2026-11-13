use krad_chat::gateway_models;
use krad_domain::{GatewayAccess, GatewayKey, GatewayModel, GatewayUrl};
use krad_protocol::{EventKind, GatewayCommand, GatewayState, Reply};
use krad_store::{delete_gateway_key, find_gateway_key, write_gateway_key};

use crate::jobs::{JobDone, Jobs};

const JOB_FAILED: &str = "the Gateway check could not start";

#[derive(Debug)]
pub(crate) struct GatewayRead {
    attempt: u64,
    access: Option<GatewayAccess>,
    state: GatewayState,
}

#[derive(Debug)]
pub(crate) struct GatewayForgotten {
    removed: Result<(), String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GatewayReached<'a> {
    pub(crate) access: &'a GatewayAccess,
    pub(crate) models: &'a [GatewayModel],
}

#[derive(Debug, Default)]
pub(crate) struct Gateway {
    attempt: u64,
    pub(crate) access: Option<GatewayAccess>,
    state: GatewayState,
}

fn failed(has_key: bool, error: String) -> GatewayState {
    GatewayState::Failed { has_key, error }
}

fn check(url: GatewayUrl, key: Option<&GatewayKey>) -> (Option<GatewayAccess>, GatewayState) {
    if let Some(key) = key {
        if let Err(error) = write_gateway_key(&url, key) {
            return (None, failed(false, error.to_string()));
        }
    }
    let (key, place) = match find_gateway_key(&url) {
        Ok(Some(found)) => found,
        Ok(None) => return (None, GatewayState::Unset),
        Err(error) => return (None, failed(false, error.to_string())),
    };
    let access = GatewayAccess { url, key };
    match gateway_models(&access) {
        Ok(models) => (Some(access), GatewayState::Ready { place, models }),
        Err(error) => (None, failed(true, error.to_string())),
    }
}

impl Gateway {
    pub(crate) fn execute(
        &mut self,
        command: GatewayCommand,
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) -> Option<Reply> {
        match command {
            GatewayCommand::Check { url: None, .. } => {
                self.restart();
                self.tell(GatewayState::Unset, outbox);
            }
            GatewayCommand::Check {
                url: Some(url),
                key,
            } => {
                self.restart();
                let attempt = self.attempt;
                let started = jobs.run(move || {
                    let (access, state) = check(url, key.as_ref());
                    JobDone::GatewayChecked(GatewayRead {
                        attempt,
                        access,
                        state,
                    })
                });
                let state = match started {
                    Ok(()) => GatewayState::Checking,
                    Err(error) => failed(false, format!("{JOB_FAILED}: {error}")),
                };
                self.tell(state, outbox);
            }
            GatewayCommand::ForgetKey { url } => {
                self.restart();
                let started = jobs.run(move || {
                    let removed = delete_gateway_key(&url).map_err(|error| error.to_string());
                    JobDone::GatewayForgotten(GatewayForgotten { removed })
                });
                if let Err(error) = started {
                    outbox.push(EventKind::GatewayKeyForgotten {
                        error: Some(error.to_string()),
                    });
                }
            }
            GatewayCommand::Unknown => return None,
        }
        Some(Reply::Accepted)
    }

    fn restart(&mut self) {
        self.attempt += 1;
        self.access = None;
    }

    fn tell(&mut self, state: GatewayState, outbox: &mut Vec<EventKind>) {
        self.state = state.clone();
        outbox.push(EventKind::GatewayChanged { state });
    }

    pub(crate) fn checked(&mut self, read: GatewayRead, outbox: &mut Vec<EventKind>) -> bool {
        if read.attempt != self.attempt {
            return false;
        }
        self.access = read.access;
        self.tell(read.state, outbox);
        self.access.is_some()
    }

    pub(crate) fn forgotten(forgotten: GatewayForgotten, outbox: &mut Vec<EventKind>) {
        outbox.push(EventKind::GatewayKeyForgotten {
            error: forgotten.removed.err(),
        });
    }

    pub(crate) fn reached(&self) -> Option<GatewayReached<'_>> {
        let models = match &self.state {
            GatewayState::Ready { models, .. } => models.as_slice(),
            GatewayState::Unset | GatewayState::Checking | GatewayState::Failed { .. } => &[],
        };
        self.access
            .as_ref()
            .map(|access| GatewayReached { access, models })
    }

    pub(crate) fn snapshot(&self) -> GatewayState {
        self.state.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access() -> Option<GatewayAccess> {
        Some(GatewayAccess {
            url: "http://127.0.0.1:1".parse().ok()?,
            key: GatewayKey::parse("sk-test")?,
        })
    }

    #[test]
    fn only_the_latest_check_keeps_its_access_and_tells_its_state() {
        let mut gateway = Gateway {
            attempt: 2,
            ..Gateway::default()
        };
        let mut outbox = Vec::new();
        let ready = GatewayState::Ready {
            place: krad_domain::LoginPlace::Keyring,
            models: Vec::new(),
        };
        let stale = GatewayRead {
            attempt: 1,
            access: access(),
            state: ready.clone(),
        };
        assert!(!gateway.checked(stale, &mut outbox));
        assert_eq!(gateway.access, None);
        assert_eq!(outbox, Vec::new());
        let latest = GatewayRead {
            attempt: 2,
            access: access(),
            state: ready.clone(),
        };
        assert!(gateway.checked(latest, &mut outbox));
        assert_eq!(gateway.access, access());
        assert_eq!(
            outbox,
            [EventKind::GatewayChanged {
                state: ready.clone()
            }]
        );
        assert_eq!(gateway.snapshot(), ready);
        assert!(!format!("{outbox:?}").contains("sk-test"));
    }
}
