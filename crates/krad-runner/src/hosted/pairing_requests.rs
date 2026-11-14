use krad_protocol::{ErrorCode, Message, PairingAnswer, PairingCommand, Reply, RequestId};

use super::connection::{refused, Peer};
use super::listener::Stream;
use super::permits::needed_for_pairing;
use super::server::RuntimeServer;
use crate::pairings::{pair, paired_devices};

const NOT_ALLOWED: &str = "pairing other devices needs the admin permission";
const MORE_THAN_HELD: &str = "a pairing code cannot grant more than its maker holds";
const NO_CODE: &str = "the cloud runner could not make a pairing code";
const UNKNOWN_CODE: &str = "that pairing code is unknown, used or expired";
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

pub(crate) fn answer_pairing<S: Stream>(
    server: &RuntimeServer<S>,
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
        } => {
            let Some(permissions) = server.codes.redeem(&code) else {
                return no(ErrorCode::NotFound, UNKNOWN_CODE);
            };
            match pair(&server.root, &install, device, permissions.clone()) {
                Ok(token) => {
                    tracing::info!(install = install.as_str(), "paired a device with a code");
                    PairingOutcome::Answered(PairingAnswer::Redeemed { token, permissions })
                }
                Err(error) => {
                    tracing::warn!(%error, "a redeemed pairing could not be kept");
                    no(ErrorCode::Failed, NOT_KEPT)
                }
            }
        }
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
