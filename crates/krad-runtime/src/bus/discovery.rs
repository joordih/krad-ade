use krad_agents::{ApiDiscovery, ApiFile};
use krad_domain::ApiDiscoveryFile;
use krad_protocol::{ErrorCode, Reply};

use super::access::BusAccess;

const NO_CONFIG_FOLDER: &str = "the app has no config folder for the discovery file";

#[tracing::instrument(skip_all, fields(?wanted))]
pub(crate) fn publish(access: &BusAccess, wanted: ApiDiscoveryFile) -> Reply {
    let Some(dir) = &access.config_dir else {
        return Reply::Refused {
            code: ErrorCode::Unsupported,
            message: NO_CONFIG_FOLDER.to_owned(),
        };
    };
    let file = ApiFile::in_config(dir);
    let done = match wanted {
        ApiDiscoveryFile::On => file.write(&ApiDiscovery::new(&access.endpoint, access.started_at)),
        ApiDiscoveryFile::Off => file.remove(),
    };
    match done {
        Ok(()) => tracing::info!("agents API discovery file updated"),
        Err(error) => tracing::warn!(%error, "agents API discovery file not updated"),
    }
    Reply::Accepted
}

#[tracing::instrument(skip_all)]
pub(crate) fn withdraw(access: &BusAccess) {
    let Some(dir) = &access.config_dir else {
        return;
    };
    if let Err(error) = ApiFile::in_config(dir).remove_if_written_by(std::process::id()) {
        tracing::warn!(%error, "agents API discovery file left behind");
    }
}
