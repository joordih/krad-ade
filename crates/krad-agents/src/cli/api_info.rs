use super::exit::Exit;
use super::{emit, Output};
use crate::client::call;
use crate::discovery::{liveness, Liveness};
use crate::env::{BusEnv, BusSource};
use crate::handle::Caller;
use crate::protocol::{ApiInfo, ApiVersion, BusToken, Request, Response};
use serde::Serialize;
use std::path::PathBuf;

const FROM_ENVIRONMENT: &str = "the KRAD_ADE_* environment variables of this terminal";
const NOT_ANSWERING: &str = "is stale: no app answers at its endpoint";
const OTHER_PROCESS: &str = "is stale: another process answers at its endpoint";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum TokenDisplay {
    #[default]
    Hidden,
    Shown,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiDetails {
    endpoint: String,
    api_version: ApiVersion,
    build: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    discovery_file: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<BusToken>,
}

pub(super) fn run(bus: &BusEnv, from: &Caller, token: TokenDisplay, json: bool) -> i32 {
    let info = match call(&bus.endpoint, &bus.token, from, Request::Hello) {
        Ok(Response::Hello(info)) => Some(info),
        Ok(Response::Error(error)) => {
            emit(Output::Stderr(error.message));
            return Exit::from(error.code).code();
        }
        Ok(_) | Err(_) => None,
    };
    if let Err(stale) = check_live(&bus.source, info.as_ref()) {
        emit(Output::Stderr(stale));
        return Exit::Bus.code();
    }
    let Some(info) = info else {
        emit(Output::Stderr(
            crate::error::AgentsError::ConnectionClosed.to_string(),
        ));
        return Exit::Bus.code();
    };
    let details = ApiDetails {
        endpoint: bus.endpoint.clone(),
        api_version: info.api_version,
        build: info.build,
        discovery_file: match &bus.source {
            BusSource::Environment => None,
            BusSource::ApiFile { path, .. } => Some(path.clone()),
        },
        token: (token == TokenDisplay::Shown).then(|| bus.token.clone()),
    };
    let output = if json {
        serde_json::to_string(&details)
            .map_or_else(|error| Output::Stderr(error.to_string()), Output::Stdout)
    } else {
        Output::Stdout(render(&details))
    };
    emit(output);
    Exit::Ok.code()
}

pub(super) fn check_live(source: &BusSource, info: Option<&ApiInfo>) -> Result<(), String> {
    let BusSource::ApiFile { path, pid } = source else {
        return Ok(());
    };
    let path = path.display();
    match liveness(*pid, info) {
        Liveness::Live => Ok(()),
        Liveness::NotAnswering => Err(format!(
            "The discovery file {path} {NOT_ANSWERING} (pid {pid})"
        )),
        Liveness::OtherProcess => Err(format!(
            "The discovery file {path} {OTHER_PROCESS} (pid {pid})"
        )),
    }
}

fn render(details: &ApiDetails) -> String {
    let source = details.discovery_file.as_ref().map_or_else(
        || FROM_ENVIRONMENT.to_owned(),
        |path| format!("the discovery file {}", path.display()),
    );
    let mut lines = vec![
        format!("Endpoint: {}", details.endpoint),
        format!(
            "Agents API: {} (build {})",
            details.api_version, details.build
        ),
        format!("Found through: {source}"),
    ];
    if let Some(token) = &details.token {
        lines.push(format!("Token: {}", token.as_str()));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::API_VERSION;

    const PID: u32 = 4242;

    fn details(token: Option<BusToken>) -> ApiDetails {
        ApiDetails {
            endpoint: "127.0.0.1:40123".to_owned(),
            api_version: API_VERSION,
            build: "0.3.3".to_owned(),
            discovery_file: Some(PathBuf::from("/config/krad-ade/api.json")),
            token,
        }
    }

    #[test]
    fn the_token_shows_only_when_asked() -> Result<(), serde_json::Error> {
        let hidden = render(&details(None));
        assert!(hidden.contains("Endpoint: 127.0.0.1:40123"));
        assert!(hidden.contains("Agents API: v1 (build 0.3.3)"));
        assert!(!hidden.contains("Token"));
        assert!(!serde_json::to_string(&details(None))?.contains("token"));
        let shown = render(&details(Some(BusToken::from("secret"))));
        assert!(shown.ends_with("Token: secret"));
        assert!(
            serde_json::to_string(&details(Some(BusToken::from("secret"))))?
                .contains("\"token\":\"secret\"")
        );
        Ok(())
    }

    #[test]
    fn a_discovery_file_is_trusted_only_while_its_app_answers() {
        let file = BusSource::ApiFile {
            path: PathBuf::from("/config/krad-ade/api.json"),
            pid: PID,
        };
        let answer = |pid| ApiInfo {
            pid: Some(pid),
            ..ApiInfo::current()
        };
        assert_eq!(check_live(&file, Some(&answer(PID))), Ok(()));
        assert!(check_live(&file, None).is_err_and(|error| error.contains(NOT_ANSWERING)));
        assert!(check_live(&file, Some(&answer(PID + 1)))
            .is_err_and(|error| error.contains(OTHER_PROCESS)));
        assert_eq!(check_live(&BusSource::Environment, None), Ok(()));
    }
}
