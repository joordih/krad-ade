use krad_domain::{LocatorId, Toolchain};
use krad_host::toolchain_cli::{FOLDER_FLAG, PYTHON_SUBJECT, ROOT_FLAG, TOOLCHAINS_VERB};
use krad_host::HostCommand;

use super::locate::LocateContext;

pub(crate) fn run_pet(context: &LocateContext<'_>, id: &LocatorId) -> Vec<Toolchain> {
    let Some(cli) = context.host_cli else {
        tracing::debug!("the host has no bundled CLI to detect Python environments");
        return Vec::new();
    };
    let mut command = HostCommand::new(cli);
    command
        .args([TOOLCHAINS_VERB, PYTHON_SUBJECT, ROOT_FLAG])
        .arg(context.group_root)
        .arg(FOLDER_FLAG)
        .arg(context.project_folder)
        .in_login_shell();
    let Some(text) = context.output_text(&command) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<Toolchain>>(&text) {
        Ok(found) => found
            .into_iter()
            .map(|toolchain| Toolchain {
                family: context.family.clone(),
                locator: id.clone(),
                ..toolchain
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "the Python environment detector answered with unreadable data");
            Vec::new()
        }
    }
}
