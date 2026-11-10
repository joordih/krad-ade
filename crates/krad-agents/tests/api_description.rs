use krad_agents::{
    ApiDiscovery, Envelope, ErrorCode, Response, StateReason, TerminalState, API_VERSION,
};
use krad_domain::AgentState;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

const DESCRIPTION: &str = include_str!("../../../sdk/schema/krad-agents-api.json");

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn description() -> serde_json::Result<Value> {
    serde_json::from_str(DESCRIPTION)
}

fn exact<T: DeserializeOwned + Serialize>(example: &Value) -> TestResult {
    let parsed: T = serde_json::from_value(example.clone())?;
    let written = serde_json::to_value(&parsed)?;
    assert_eq!(
        &written, example,
        "the description must match the wire exactly"
    );
    Ok(())
}

fn entries(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_object().into_iter().flat_map(|map| map.values())
}

fn names(value: &Value) -> Vec<&str> {
    value.as_array().map_or_else(Vec::new, |items| {
        items.iter().filter_map(Value::as_str).collect()
    })
}

#[test]
fn every_example_line_reads_and_writes_back_as_itself() -> TestResult {
    let api = description()?;
    assert_eq!(api["api_version"], API_VERSION.0);
    exact::<ApiDiscovery>(&api["discovery"]["example"])?;
    for request in entries(&api["requests"]) {
        let envelope: Envelope = serde_json::from_value(request.clone())?;
        assert_eq!(envelope.api_version, API_VERSION);
    }
    for response in entries(&api["responses"]) {
        exact::<Response>(response)?;
    }
    let session = api["example_session"].as_array().ok_or("a session")?;
    assert!(session.len() > 2);
    for step in session {
        match step["from"].as_str() {
            Some("client") => {
                serde_json::from_value::<Envelope>(step["line"].clone())?;
            }
            Some("app") => exact::<Response>(&step["line"])?,
            other => return Err(format!("unknown side {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn the_listed_codes_states_and_reasons_are_the_ones_the_api_speaks() -> TestResult {
    let api = description()?;
    let codes: Vec<&str> = ErrorCode::ALL.iter().map(|code| code.name()).collect();
    assert_eq!(names(&api["error_codes"]), codes);
    let reasons: Vec<&str> = StateReason::ALL
        .iter()
        .map(|reason| reason.name())
        .collect();
    assert_eq!(names(&api["reasons"]), reasons);
    let states: Vec<String> = AgentState::ALL
        .into_iter()
        .map(TerminalState::Running)
        .chain([TerminalState::Exited, TerminalState::Closed])
        .map(|state| state.to_string())
        .collect();
    let mut listed: Vec<&str> = names(&api["states"]);
    listed.sort_unstable();
    let mut spoken: Vec<&str> = states.iter().map(String::as_str).collect();
    spoken.sort_unstable();
    assert_eq!(listed, spoken);
    for reason in StateReason::ALL {
        assert_eq!(serde_json::to_value(reason)?, reason.name());
    }
    Ok(())
}
