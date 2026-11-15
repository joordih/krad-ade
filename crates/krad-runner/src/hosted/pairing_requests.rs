use krad_protocol::{ErrorCode, Message, PairingAnswer, PairingCommand, Reply, RequestId};

use super::connection::{refused, Peer};
use super::permits::needed_for_pairing;
use super::server::{RedeemRefusal, RuntimeServer};
use crate::pairings::paired_devices;

const NOT_ALLOWED: &str = "pairing other devices needs the admin permission";
const MORE_THAN_HELD: &str = "a pairing code cannot grant more than its maker holds";
const NO_CODE: &str = "the cloud runner could not make a pairing code";
const NOT_KEPT: &str = "the cloud runner could not keep the pairing";
const NO_PAIRING: &str = "no installation with that id is paired";
const UNKNOWN_VERB: &str = "this runtime does not know that pairing command";

pub(crate) enum PairingOutcome {
    Answered(PairingAnswer),
    Refused(Reply),
}

impl PairingOutcome {
    pub(crate) fn into_message(self, id: RequestId) -> Message {
        match self {
            Self::Answered(answer) => Message::Paired { id, answer },
            Self::Refused(reply) => Message::Reply { id, reply },
        }
    }
}

fn no(code: ErrorCode, message: &str) -> PairingOutcome {
    PairingOutcome::Refused(refused(code, message))
}

fn redeem_refused(refusal: &RedeemRefusal) -> PairingOutcome {
    let code = match refusal {
        RedeemRefusal::UnknownCode => ErrorCode::NotFound,
        RedeemRefusal::TooManyTries => ErrorCode::Busy,
        RedeemRefusal::NotKept(error) => {
            tracing::warn!(%error, "a redeemed pairing could not be kept");
            ErrorCode::Failed
        }
    };
    no(code, &refusal.to_string())
}

pub(crate) fn answer_pairing(
    server: &RuntimeServer,
    peer: &Peer,
    command: PairingCommand,
) -> PairingOutcome {
    if let Some(needed) = needed_for_pairing(&command) {
        if !peer.allows(needed) {
            return no(ErrorCode::Forbidden, NOT_ALLOWED);
        }
    }
    match command {
        PairingCommand::CreatePairingCode { permissions } => {
            if !peer.permissions().covers(&permissions) {
                return no(ErrorCode::Forbidden, MORE_THAN_HELD);
            }
            match server.codes.offer(permissions) {
                Some((code, expires)) => {
                    PairingOutcome::Answered(PairingAnswer::Code { code, expires })
                }
                None => no(ErrorCode::Failed, NO_CODE),
            }
        }
        PairingCommand::RedeemPairingCode {
            code,
            install,
            device,
        } => match server.redeem(&code, &install, device) {
            Ok((token, permissions)) => {
                PairingOutcome::Answered(PairingAnswer::Redeemed { token, permissions })
            }
            Err(refusal) => redeem_refused(&refusal),
        },
        PairingCommand::ListPairings => PairingOutcome::Answered(PairingAnswer::Pairings {
            devices: paired_devices(&server.root),
        }),
        PairingCommand::RevokePairing { install } => match server.revoke(&install) {
            Ok(true) => PairingOutcome::Answered(PairingAnswer::Revoked),
            Ok(false) => no(ErrorCode::NotFound, NO_PAIRING),
            Err(error) => {
                tracing::warn!(%error, "a pairing could not be revoked");
                no(ErrorCode::Failed, NOT_KEPT)
            }
        },
        PairingCommand::Unknown => no(ErrorCode::Unsupported, UNKNOWN_VERB),
    }
}
