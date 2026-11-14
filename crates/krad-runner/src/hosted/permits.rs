use krad_domain::PairingPermission;
use krad_protocol::{BusCommand, ChatCommand, CloudCommand, Command, PairingCommand};

pub(crate) const fn needed_for_command(command: &Command) -> PairingPermission {
    match command {
        Command::Runtime(_) => PairingPermission::Read,
        Command::Bus(BusCommand::AnswerAsk { .. } | BusCommand::Answer { .. })
        | Command::Chat(
            ChatCommand::AnswerAsk { .. }
            | ChatCommand::AnswerQuestion { .. }
            | ChatCommand::ApprovePlan { .. },
        )
        | Command::Cloud(CloudCommand::AnswerPatch { .. }) => PairingPermission::Answer,
        Command::Settings(_) | Command::Runner(_) => PairingPermission::Admin,
        _ => PairingPermission::Drive,
    }
}

pub(crate) const fn needed_for_pairing(command: &PairingCommand) -> Option<PairingPermission> {
    match command {
        PairingCommand::RedeemPairingCode { .. } => None,
        PairingCommand::CreatePairingCode { .. }
        | PairingCommand::ListPairings
        | PairingCommand::RevokePairing { .. }
        | PairingCommand::Unknown => Some(PairingPermission::Admin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AskAnswer, ToolDecision};
    use krad_protocol::{AskId, RuntimeCommand, SettingsCommand};

    #[test]
    fn answering_an_ask_needs_answer_and_settings_need_admin() {
        let answer = Command::Bus(BusCommand::AnswerAsk {
            ask: AskId(1),
            decision: ToolDecision::Pass,
        });
        assert_eq!(needed_for_command(&answer), PairingPermission::Answer);
        let ping = Command::Runtime(RuntimeCommand::Ping);
        assert_eq!(needed_for_command(&ping), PairingPermission::Read);
        let settings = Command::Settings(SettingsCommand::Unknown);
        assert_eq!(needed_for_command(&settings), PairingPermission::Admin);
        let answered = Command::Bus(BusCommand::Answer {
            ask: AskId(1),
            answer: AskAnswer::Allow,
        });
        assert_eq!(needed_for_command(&answered), PairingPermission::Answer);
        assert_eq!(
            needed_for_pairing(&PairingCommand::ListPairings),
            Some(PairingPermission::Admin)
        );
    }
}
