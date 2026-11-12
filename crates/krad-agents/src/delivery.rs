use crate::client::call;
use crate::env::{bus_env, BusEnv};
use crate::error::AgentsError;
use crate::handle::Caller;
use crate::offline::OfflineEvent;
use crate::protocol::{Request, Response};
use std::io::ErrorKind;

#[derive(Debug, PartialEq, Eq)]
enum Delivery {
    Answered(Response),
    Unreachable,
    Failed,
}

fn is_unreachable(error: &AgentsError) -> bool {
    match error {
        AgentsError::ConnectionClosed => true,
        AgentsError::Io(error) => matches!(
            error.kind(),
            ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::NotConnected
                | ErrorKind::BrokenPipe
                | ErrorKind::AddrNotAvailable
                | ErrorKind::UnexpectedEof
        ),
        AgentsError::Json(_) | AgentsError::NoHeartbeat | AgentsError::Host(_) => false,
    }
}

fn deliver(bus: Option<BusEnv>, request: Request) -> Delivery {
    let Some(BusEnv {
        endpoint,
        token,
        terminal,
        ..
    }) = bus
    else {
        return Delivery::Unreachable;
    };
    let Ok(from) = terminal.parse::<Caller>() else {
        return Delivery::Failed;
    };
    match call(&endpoint, &token, &from, request) {
        Ok(response) => Delivery::Answered(response),
        Err(error) if is_unreachable(&error) => Delivery::Unreachable,
        Err(_) => Delivery::Failed,
    }
}

pub(crate) fn send(request: Request) {
    ask_app(request).map_or_default(drop);
}

pub(crate) fn ask_app(request: Request) -> Option<Response> {
    let offline = OfflineEvent::of_this_process(&request);
    match deliver(bus_env(None), request) {
        Delivery::Answered(response) => Some(response),
        Delivery::Unreachable => {
            offline.map_or_default(OfflineEvent::queue);
            None
        }
        Delivery::Failed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::AgentPreset;

    #[test]
    fn a_bus_that_cannot_be_reached_counts_as_unreachable_but_a_bad_handle_does_not(
    ) -> std::io::Result<()> {
        let stop = || Request::Hook {
            harness: AgentPreset::ClaudeCode,
            event: "Stop".to_owned(),
            tool: None,
            detail: None,
            resume: None,
            mode: None,
        };
        assert_eq!(deliver(None, stop()), Delivery::Unreachable);
        let closed = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?;
        let bus = |terminal: &str| BusEnv {
            endpoint: closed.to_string(),
            token: crate::protocol::BusToken::from("secret"),
            terminal: terminal.to_owned(),
            source: crate::env::BusSource::Environment,
        };
        assert_eq!(deliver(Some(bus("api:4")), stop()), Delivery::Unreachable);
        assert_eq!(deliver(Some(bus("")), stop()), Delivery::Failed);
        assert!(!is_unreachable(&AgentsError::NoHeartbeat));
        Ok(())
    }
}
