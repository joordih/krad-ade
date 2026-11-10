use super::super::api_info::TokenDisplay;
use super::super::request::CliError;
use super::super::verb::ApiAction;
use super::request_of;
use crate::cli::agents::api_action;
use crate::cli::request::parse_options;
use crate::protocol::{AgentFilter, AgentSubscription, Request, StreamId};
use krad_domain::AgentPreset;

#[test]
fn agents_list_is_the_explicit_listing_and_keeps_list_working() -> Result<(), CliError> {
    assert_eq!(request_of("agents", &["list"])?, Some(Request::List));
    assert_eq!(
        request_of("agents", &["list", "--all", "--json"])?,
        Some(Request::ListAll)
    );
    assert_eq!(request_of("list", &[])?, Some(Request::List));
    assert_eq!(request_of("agents", &[]), Err(CliError::AgentsNeedAction));
    assert_eq!(
        request_of("agents", &["show"]),
        Err(CliError::AgentsNeedAction)
    );
    Ok(())
}

#[test]
fn agents_watch_subscribes_with_the_filter_it_was_given() -> Result<(), CliError> {
    assert_eq!(
        request_of("agents", &["watch"])?,
        Some(Request::SubscribeAgents(AgentSubscription::default()))
    );
    for harness in ["claude", "Claude Code"] {
        assert_eq!(
            request_of(
                "agents",
                &[
                    "watch",
                    "--group",
                    "api",
                    "--harness",
                    harness,
                    "--terminal",
                    "4",
                    "--since",
                    "12",
                    "--stream",
                    "ab12"
                ]
            )?,
            Some(Request::SubscribeAgents(AgentSubscription {
                filter: AgentFilter {
                    environment: None,
                    group: Some("api".to_owned()),
                    harness: Some(AgentPreset::ClaudeCode),
                    terminals: vec![4],
                },
                since_seq: Some(12),
                stream: Some(StreamId("ab12".to_owned())),
            }))
        );
    }
    assert_eq!(
        request_of("agents", &["watch", "--harness", "Shell"])?,
        Some(Request::SubscribeAgents(AgentSubscription {
            filter: AgentFilter {
                harness: Some(AgentPreset::Shell),
                ..AgentFilter::default()
            },
            ..AgentSubscription::default()
        }))
    );
    assert_eq!(
        request_of("agents", &["watch", "--harness", "vim"]),
        Err(CliError::UnknownHarness("vim".to_owned()))
    );
    assert!(matches!(
        request_of("agents", &["watch", "--since", "soon"]),
        Err(CliError::InvalidValue { .. })
    ));
    Ok(())
}

#[test]
fn api_info_is_the_only_api_action() -> Result<(), CliError> {
    let words = |text: &[&str]| {
        text.iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        api_action(&parse_options(&words(&[]))?),
        Ok(ApiAction::Version)
    );
    let info = parse_options(&words(&["info", "--show-token"]))?;
    assert_eq!(api_action(&info), Ok(ApiAction::Info));
    assert_eq!(info.token, TokenDisplay::Shown);
    assert_eq!(request_of("api", &["info"])?, Some(Request::Hello));
    assert_eq!(
        request_of("api", &["tokens"]),
        Err(CliError::ApiNeedsAction)
    );
    Ok(())
}
