use super::*;
use krad_chat::BusVars;
use krad_domain::GatewayModel;
use krad_host::HostLink;
use std::path::PathBuf;

fn waiting(_: &LiveConfig) -> Reach {
    Reach::Waiting
}

fn codex(model: Option<&str>) -> ChatLaunch {
    ChatLaunch::from(ChatBackend::Codex(LiveConfig {
        runner: Runner::Native(PathBuf::from("codex")),
        model: model.and_then(ModelId::parse),
        cwd: PathBuf::from("/work"),
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
        usage_budget: std::sync::Arc::default(),
    }))
}

#[test]
fn a_restarted_chat_starts_with_the_choices_made_since_it_opened() {
    let template = codex(Some("gpt-5"));
    let mut choices = Choices::from(&template);
    assert_eq!(choices.model, ModelId::parse("gpt-5"));
    choices.model = ModelId::parse("gpt-5-codex");
    choices.effort = Some(Effort::High);
    let Ok(ChatBackend::Codex(config)) = chosen(template, &choices).resolve(None, waiting) else {
        unreachable!("the harness does not change");
    };
    assert_eq!(config.model, ModelId::parse("gpt-5-codex"));
    assert_eq!(config.effort, Some(Effort::High));
}

#[test]
fn a_gateway_launch_takes_the_runtime_access_and_served_models_and_waits_without_them() {
    let Ok(ChatBackend::Codex(config)) = codex(None).resolve(None, waiting) else {
        unreachable!("the template is a Codex chat");
    };
    let engine = EngineConfig {
        host: HostLink::Local,
        sessions: PathBuf::from("/sessions"),
        home: None,
        cwd: config.cwd,
        model: None,
        models: Vec::new(),
        context_windows: std::collections::HashMap::new(),
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
        mcp: Vec::new(),
        skills: krad_domain::SkillSettings::default(),
    };
    let launch = ChatLaunch::Gateway(Box::new(engine.clone()));
    assert_eq!(
        launch.clone().resolve(None, waiting).err(),
        Some(Unready::Problem(GATEWAY_UNREACHED.to_owned()))
    );
    let (Ok(url), Some(key)) = (
        "http://127.0.0.1:1".parse(),
        krad_domain::GatewayKey::parse("sk-test"),
    ) else {
        unreachable!("a loopback address and a non-blank key");
    };
    let access = krad_domain::GatewayAccess { url, key };
    let served = [GatewayModel {
        offer: krad_domain::ModelOffer {
            value: "gemini-3-pro".to_owned(),
            ..krad_domain::ModelOffer::default()
        },
        vendor: krad_domain::Vendor::Google,
    }];
    let settings = krad_domain::GatewaySettings::default();
    let reached = GatewayReached {
        access: &access,
        models: &served,
        settings: &settings,
    };
    let Ok(ChatBackend::Gateway(used, started)) = launch.clone().resolve(Some(reached), waiting)
    else {
        unreachable!("a reached Gateway starts its chat");
    };
    assert_eq!(used, access);
    assert_eq!(started.models, served);
    assert_eq!(started.model, ModelId::parse("gemini-3-pro"));
    assert_eq!(started.cwd, engine.cwd);
    let offered = ModelId::parse("gpt-6");
    let mut settings = krad_domain::GatewaySettings::default();
    settings.models.extend(offered.clone());
    settings.context_windows.extend(
        offered
            .clone()
            .map(|model| (model, krad_domain::ContextWindow::default())),
    );
    let mut both = served.to_vec();
    both.push(GatewayModel {
        offer: krad_domain::ModelOffer {
            value: "gpt-6".to_owned(),
            ..krad_domain::ModelOffer::default()
        },
        vendor: krad_domain::Vendor::OpenAi,
    });
    let reached = GatewayReached {
        access: &access,
        models: &both,
        settings: &settings,
    };
    let Ok(ChatBackend::Gateway(_, started)) = launch.resolve(Some(reached), waiting) else {
        unreachable!("a reached Gateway starts its chat");
    };
    assert_eq!(started.model, offered);
    assert_eq!(started.context_windows, settings.context_windows);
}

#[test]
fn only_harnesses_moved_to_the_runtime_run_there() {
    assert!(runs_in_runtime(AgentPreset::ClaudeCode));
    assert!(runs_in_runtime(AgentPreset::Codex));
    assert!(runs_in_runtime(AgentPreset::Gateway));
    assert!(runs_in_runtime(AgentPreset::Opencode));
    assert!(runs_in_runtime(AgentPreset::Acp(
        "fake"
            .parse()
            .unwrap_or_else(|_| unreachable!("a valid agent id"))
    )));
    assert!(!runs_in_runtime(AgentPreset::Shell));
}
