use crate::agent::AgentPreset;
use crate::environment::{seed_environment, EnvironmentData};
use crate::group::{SessionLead, WorkerSpawn};
use crate::ids::{GroupId, SessionId, TerminalId};
use crate::usage::{ModelPrices, ReportedUsage, Spend, TokenUsage, UsageReport, UsdCost};
use std::path::PathBuf;

fn dollars(amount: f64) -> UsdCost {
    UsdCost::try_from(amount).unwrap_or_default()
}

fn spent(amount: f64) -> UsageReport {
    UsageReport::Added(ReportedUsage {
        tokens: TokenUsage {
            output: 10,
            ..TokenUsage::default()
        },
        cost: Some(dollars(amount)),
        model: None,
    })
}

struct Led {
    environment: EnvironmentData,
    group: GroupId,
    lead: SessionId,
    lead_chat: TerminalId,
    worker: TerminalId,
}

fn led() -> Led {
    let mut environment = seed_environment();
    let group = environment.add_group(PathBuf::from("/work/api"), None);
    for node in &mut environment.nodes {
        node.group = Some(group);
    }
    environment.enter_sessions();
    let lead = environment
        .active_session
        .unwrap_or_else(|| unreachable!("Sessions mode opens a session"));
    if let Some(session) = environment
        .sessions
        .iter_mut()
        .find(|session| session.id == lead)
    {
        session.lead = Some(SessionLead::new(AgentPreset::ClaudeCode, None));
    }
    let lead_chat = environment.nodes[0].id;
    let worker = environment.nodes[1].id;
    environment.open_worker_session(
        worker,
        WorkerSpawn {
            lead: Some(lead),
            task: "Fix the retries".to_owned(),
        },
    );
    Led {
        environment,
        group,
        lead,
        lead_chat,
        worker,
    }
}

fn record(environment: &mut EnvironmentData, chat: TerminalId, amount: f64) {
    environment.record_usage(chat, &spent(amount), &ModelPrices::bundled());
}

fn session_cost(environment: &EnvironmentData, id: SessionId) -> UsdCost {
    environment.session(id).map_or(UsdCost::ZERO, |session| {
        environment.session_spend(session).cost
    })
}

#[test]
fn each_session_and_worker_shows_its_own_spend_and_the_group_their_sum() {
    let Led {
        mut environment,
        group,
        lead,
        lead_chat,
        worker,
    } = led();
    record(&mut environment, lead_chat, 1.0);
    record(&mut environment, worker, 0.5);
    record(&mut environment, worker, 0.25);
    assert_eq!(session_cost(&environment, lead), dollars(1.0));
    let worker_session = environment.session_of(worker);
    assert_eq!(
        worker_session.map(|id| session_cost(&environment, id)),
        Some(dollars(0.75))
    );
    let card = environment
        .worker(worker)
        .map(|(_, card)| environment.worker_spend(card).cost);
    assert_eq!(card, Some(dollars(0.75)));
    assert_eq!(environment.group_spend(Some(group)).cost, dollars(1.75));
    assert_eq!(environment.group_spend(Some(group)).tokens.output, 30);
    assert_eq!(environment.group_spend(None), Spend::default());
}

#[test]
fn a_closed_worker_keeps_its_spend_on_its_card_and_in_the_group() {
    let Led {
        mut environment,
        group,
        lead_chat,
        worker,
        ..
    } = led();
    record(&mut environment, lead_chat, 1.0);
    record(&mut environment, worker, 0.5);
    environment.retire_chat_usage(worker);
    environment.retire_worker(worker);
    environment.nodes.retain(|node| node.id != worker);
    environment.remove_terminal_tiles(worker);
    let card = environment
        .worker(worker)
        .map(|(_, card)| environment.worker_spend(card).cost);
    assert_eq!(card, Some(dollars(0.5)));
    assert_eq!(environment.group_spend(Some(group)).cost, dollars(1.5));
}

#[test]
fn a_reopened_worker_carries_on_from_its_spend_without_counting_it_twice() {
    let Led {
        mut environment,
        group,
        lead,
        worker,
        ..
    } = led();
    record(&mut environment, worker, 0.5);
    environment.retire_chat_usage(worker);
    environment.retire_worker(worker);
    environment.open_worker_session(
        worker,
        WorkerSpawn {
            lead: Some(lead),
            task: "Fix the retries".to_owned(),
        },
    );
    environment.resume_worker_usage(worker);
    assert_eq!(environment.group_spend(Some(group)).cost, dollars(0.5));
    record(&mut environment, worker, 0.25);
    assert_eq!(environment.group_spend(Some(group)).cost, dollars(0.75));
    assert_eq!(environment.chat_spend(worker).cost, dollars(0.75));
}

#[test]
fn removing_a_group_forgets_what_its_closed_chats_spent() {
    let Led {
        mut environment,
        group,
        worker,
        ..
    } = led();
    record(&mut environment, worker, 0.5);
    environment.retire_chat_usage(worker);
    assert!(!environment.retired_spend.is_empty());
    environment.remove_group(group);
    assert!(environment.retired_spend.is_empty());
}

#[test]
fn spend_survives_a_save_and_a_layout_saved_without_it_still_loads() {
    let Led {
        mut environment,
        lead_chat,
        worker,
        ..
    } = led();
    record(&mut environment, lead_chat, 1.0);
    record(&mut environment, worker, 0.5);
    environment.retire_chat_usage(worker);
    let saved = serde_json::to_value(&environment).unwrap_or_default();
    let loaded: Option<EnvironmentData> = serde_json::from_value(saved).ok();
    let loaded = loaded.unwrap_or_else(|| unreachable!("the layout reloads"));
    assert_eq!(loaded.chat_spend(lead_chat).cost, dollars(1.0));
    assert_eq!(loaded.retired_spend, environment.retired_spend);
    let bare = serde_json::to_value(seed_environment()).unwrap_or_default();
    assert!(bare.get("retired_spend").is_none());
    assert!(serde_json::from_value::<EnvironmentData>(bare).is_ok());
}
