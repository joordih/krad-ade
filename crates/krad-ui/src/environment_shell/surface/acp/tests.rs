use super::*;
use krad_domain::{AcpConfigCategory, AcpConfigChoice, AcpMode, TerminalId};

fn mode(id: &str) -> AcpMode {
    AcpMode {
        id: AcpModeId(id.to_owned()),
        name: id.to_uppercase(),
        description: None,
    }
}

fn modes(current: &str, available: &[&str]) -> AcpModes {
    AcpModes {
        current: AcpModeId(current.to_owned()),
        available: available.iter().map(|id| mode(id)).collect(),
    }
}

fn heard(offers: &mut AcpOffers, event: AcpEvent) {
    assert_eq!(
        offers.reported(event),
        Vec::<AcpChoice>::new(),
        "nothing was chosen before the agent reported"
    );
}

fn shown_mode(offers: &AcpOffers) -> Option<String> {
    offers.mode_picker(TerminalId(1)).map(|picker| picker.label)
}

#[test]
fn the_mode_picker_shows_the_agent_s_current_mode_and_switches_to_an_offered_one() {
    let mut offers = AcpOffers::default();
    assert_eq!(
        shown_mode(&offers),
        None,
        "no picker before the agent reports modes"
    );
    heard(&mut offers, AcpEvent::Modes(modes("ask", &["ask", "code"])));
    assert_eq!(shown_mode(&offers).as_deref(), Some("ASK"));
    assert!(offers.chose(&AcpChoice::Mode(AcpModeId("code".to_owned()))));
    assert_eq!(shown_mode(&offers).as_deref(), Some("CODE"));
    assert!(!offers.chose(&AcpChoice::Mode(AcpModeId("plan".to_owned()))));
    heard(&mut offers, AcpEvent::Modes(modes("ask", &[])));
    assert_eq!(shown_mode(&offers), None, "an empty list hides the picker");
}

#[test]
fn a_config_option_becomes_its_own_picker_and_only_takes_its_own_values() {
    let mut offers = AcpOffers::default();
    let choice = |value: &str| AcpConfigChoice {
        value: AcpConfigValue(value.to_owned()),
        name: value.to_uppercase(),
        description: None,
    };
    heard(
        &mut offers,
        AcpEvent::Options(vec![AcpConfigOption {
            id: AcpConfigOptionId("effort".to_owned()),
            name: "Thought level".to_owned(),
            description: None,
            category: None,
            current: AcpConfigValue("low".to_owned()),
            choices: vec![choice("low"), choice("high")],
        }]),
    );
    let labels = |offers: &AcpOffers| -> Vec<String> {
        offers
            .option_pickers(TerminalId(1))
            .map(|picker| picker.label)
            .collect()
    };
    assert_eq!(labels(&offers), ["LOW"]);
    let effort = AcpConfigOptionId("effort".to_owned());
    assert!(offers.chose(&AcpChoice::Option(
        effort.clone(),
        AcpConfigValue("high".to_owned())
    )));
    assert_eq!(labels(&offers), ["HIGH"]);
    assert!(!offers.chose(&AcpChoice::Option(effort, AcpConfigValue("max".to_owned()))));
    assert!(offers.live_models().is_none());
}

fn switch_option(
    id: &str,
    category: Option<AcpConfigCategory>,
    current: &str,
    values: &[&str],
) -> AcpConfigOption {
    AcpConfigOption {
        id: AcpConfigOptionId(id.to_owned()),
        name: id.to_owned(),
        description: None,
        category,
        current: AcpConfigValue(current.to_owned()),
        choices: values
            .iter()
            .map(|value| AcpConfigChoice {
                value: AcpConfigValue((*value).to_owned()),
                name: value.to_uppercase(),
                description: None,
            })
            .collect(),
    }
}

#[test]
fn claude_s_effort_ultracode_fast_and_persona_leave_the_pills_for_the_effort_slider() {
    let mut offers = AcpOffers::default();
    heard(
        &mut offers,
        AcpEvent::Options(vec![
            switch_option(
                "effort",
                Some(AcpConfigCategory::ThoughtLevel),
                "medium",
                &["default", "low", "medium", "high", "xhigh", "max"],
            ),
            switch_option(
                "fast",
                Some(AcpConfigCategory::ModelConfig),
                "off",
                &["on", "off"],
            ),
            switch_option(
                "ultracode",
                Some(AcpConfigCategory::ModelConfig),
                "off",
                &["on", "off"],
            ),
            switch_option("agent", None, "default", &["default", "reviewer"]),
        ]),
    );
    assert_eq!(offers.option_pickers(TerminalId(1)).count(), 0);
    let effort = offers
        .effort()
        .unwrap_or_else(|| unreachable!("the agent reported an effort"));
    assert_eq!(effort.levels.last(), Some(&Effort::Ultra));
    assert_eq!(effort.current, Some(Effort::Medium));
    assert_eq!(effort.fast, Some(false));
    let ultra = offers.effort_choices(Effort::Ultra);
    for choice in &ultra {
        assert!(offers.chose(choice));
    }
    assert_eq!(
        offers.effort().and_then(|effort| effort.current),
        Some(Effort::Ultra)
    );
    let fast = offers.fast_choice(true);
    assert_eq!(
        fast,
        Some(AcpChoice::Option(
            AcpConfigOptionId("fast".to_owned()),
            AcpConfigValue("on".to_owned())
        ))
    );
}

#[test]
fn a_second_sign_in_ask_after_choosing_marks_it_failed_and_opening_clears_it() {
    let mut offers = AcpOffers::default();
    let methods = vec![krad_domain::AcpAuthMethod {
        id: krad_domain::AcpAuthMethodId("login".to_owned()),
        name: "Log in".to_owned(),
        description: None,
        sign_in: krad_domain::AcpSignIn::ByAgent,
    }];
    heard(&mut offers, AcpEvent::SignInNeeded(methods.clone()));
    let asked = offers.sign_in.as_mut().map(|asked| {
        asked.step = sign_in::SignInStep::Signing(methods[0].id.clone());
    });
    assert!(asked.is_some());
    heard(&mut offers, AcpEvent::SignInNeeded(methods.clone()));
    assert_eq!(
        offers.sign_in.as_ref().map(|asked| asked.step.clone()),
        Some(sign_in::SignInStep::Failed(methods[0].id.clone()))
    );
    offers.opened();
    assert!(offers.sign_in.is_none());
}

#[test]
fn the_models_an_agent_reports_become_the_chat_s_model_rows_with_its_current_one() {
    let mut offers = AcpOffers::default();
    let model = |id: &str, name: &str| krad_domain::AcpModel {
        id: krad_domain::AcpModelId(id.to_owned()),
        name: name.to_owned(),
        description: None,
    };
    heard(
        &mut offers,
        AcpEvent::Models(AcpModels {
            current: krad_domain::AcpModelId("opus".to_owned()),
            available: vec![model("default", "Default"), model("opus", "Opus")],
        }),
    );
    let (rows, current) = offers
        .live_models()
        .unwrap_or_else(|| unreachable!("the agent reported models"));
    let shown: Vec<(&str, &str)> = rows
        .iter()
        .map(|offer| (offer.value.as_str(), offer.name.as_str()))
        .collect();
    assert_eq!(shown, [("default", "Default"), ("opus", "Opus")]);
    assert_eq!(current.as_ref().map(ModelId::as_str), Some("opus"));
}

fn claude_seed(effort: &str) -> AcpSeed {
    AcpSeed::from(&AcpOffered {
        models: Vec::new(),
        modes: Some(modes("default", &["default", "plan"])),
        options: vec![switch_option(
            "effort",
            Some(AcpConfigCategory::ThoughtLevel),
            effort,
            &["low", "medium", "high"],
        )],
    })
}

#[test]
fn a_seeded_chat_shows_the_agent_s_mode_and_effort_before_its_session_opens() {
    let mut offers = AcpOffers::default();
    assert_eq!(shown_mode(&offers), None);
    assert_eq!(offers.effort(), None);
    offers.seed(&claude_seed("medium"));
    assert_eq!(shown_mode(&offers).as_deref(), Some("DEFAULT"));
    assert_eq!(
        offers.effort().and_then(|effort| effort.current),
        Some(Effort::Medium)
    );
    assert_eq!(offers.wanted(), AcpWanted::default());
}

#[test]
fn choices_made_before_the_session_opened_are_wanted_once_each_and_resent_on_its_first_report() {
    let mut offers = AcpOffers::default();
    offers.seed(&claude_seed("medium"));
    let plan = AcpChoice::Mode(AcpModeId("plan".to_owned()));
    assert!(offers.chose(&plan));
    for choice in offers
        .effort_choices(Effort::Low)
        .into_iter()
        .chain(offers.effort_choices(Effort::High))
    {
        assert!(offers.chose(&choice));
    }
    let high = (
        AcpConfigOptionId("effort".to_owned()),
        AcpConfigValue("high".to_owned()),
    );
    assert_eq!(
        offers.wanted(),
        AcpWanted {
            mode: Some(AcpModeId("plan".to_owned())),
            options: vec![high.clone()],
        }
    );
    offers.seed(&claude_seed("low"));
    assert_eq!(
        offers.effort().and_then(|effort| effort.current),
        Some(Effort::High),
        "a late probe never undoes what the user chose"
    );
    assert_eq!(
        offers.reported(AcpEvent::Modes(modes("plan", &["default", "plan"]))),
        [plan, AcpChoice::Option(high.0, high.1)]
    );
    assert_eq!(shown_mode(&offers).as_deref(), Some("PLAN"));
    assert_eq!(
        offers.effort().and_then(|effort| effort.current),
        Some(Effort::High),
        "the seeded effort stays until the agent reports its own"
    );
    assert_eq!(offers.wanted(), AcpWanted::default());
}

#[test]
fn a_seed_never_replaces_what_the_agent_reported() {
    let mut offers = AcpOffers::default();
    heard(&mut offers, AcpEvent::Modes(modes("code", &["code"])));
    offers.seed(&claude_seed("medium"));
    assert_eq!(shown_mode(&offers).as_deref(), Some("CODE"));
    assert_eq!(offers.effort(), None);
}
