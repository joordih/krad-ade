use super::*;
use krad_domain::{AcpConfigChoice, AcpMode, TerminalId};

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
    offers.reported(AcpEvent::Modes(modes("ask", &["ask", "code"])));
    assert_eq!(shown_mode(&offers).as_deref(), Some("ASK"));
    assert!(offers.chose(&AcpChoice::Mode(AcpModeId("code".to_owned()))));
    assert_eq!(shown_mode(&offers).as_deref(), Some("CODE"));
    assert!(!offers.chose(&AcpChoice::Mode(AcpModeId("plan".to_owned()))));
    offers.reported(AcpEvent::Modes(modes("ask", &[])));
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
    offers.reported(AcpEvent::Options(vec![AcpConfigOption {
        id: AcpConfigOptionId("effort".to_owned()),
        name: "Thought level".to_owned(),
        description: None,
        current: AcpConfigValue("low".to_owned()),
        choices: vec![choice("low"), choice("high")],
    }]));
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
    assert!(!offers.has_models());
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
    offers.reported(AcpEvent::SignInNeeded(methods.clone()));
    let asked = offers.sign_in.as_mut().map(|asked| {
        asked.step = sign_in::SignInStep::Signing(methods[0].id.clone());
    });
    assert!(asked.is_some());
    offers.reported(AcpEvent::SignInNeeded(methods.clone()));
    assert_eq!(
        offers.sign_in.as_ref().map(|asked| asked.step.clone()),
        Some(sign_in::SignInStep::Failed(methods[0].id.clone()))
    );
    offers.opened();
    assert!(offers.sign_in.is_none());
}
