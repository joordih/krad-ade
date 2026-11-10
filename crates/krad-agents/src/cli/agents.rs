use super::request::{CliError, Options};
use super::verb::{AgentsAction, ApiAction};
use crate::protocol::{
    preset_by_bus_name, preset_by_display_name, AgentFilter, AgentSubscription, Request, StreamId,
};
use krad_domain::AgentPreset;

pub(super) fn agents_request(options: &Options) -> Result<Request, CliError> {
    let action = options
        .positional
        .first()
        .ok_or(CliError::AgentsNeedAction)?
        .parse::<AgentsAction>()?;
    Ok(match action {
        AgentsAction::List if options.all => Request::ListAll,
        AgentsAction::List => Request::List,
        AgentsAction::Watch => Request::SubscribeAgents(AgentSubscription {
            filter: AgentFilter {
                environment: None,
                group: options.group.clone(),
                harness: options
                    .harness
                    .as_deref()
                    .map(watched_harness)
                    .transpose()?,
                terminals: options.terminals.clone(),
            },
            since_seq: options.since,
            stream: options.stream.clone().map(StreamId),
        }),
    })
}

fn watched_harness(name: &str) -> Result<AgentPreset, CliError> {
    preset_by_bus_name(name)
        .or_else(|| preset_by_display_name(name))
        .ok_or_else(|| CliError::UnknownHarness(name.to_owned()))
}

pub(super) fn api_action(options: &Options) -> Result<ApiAction, CliError> {
    options
        .positional
        .first()
        .map_or(Ok(ApiAction::Version), |action| action.parse())
}
