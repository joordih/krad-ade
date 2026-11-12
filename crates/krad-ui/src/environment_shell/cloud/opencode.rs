use gpui_kit::{AppContext as _, Context};
use krad_chat::{
    new_opencode_password, opencode_cloud_process, opencode_models, CloudRun, OpencodeServer,
};
use krad_domain::{AgentPreset, CloudRunnerId, JournalOffset, OpencodePassword};
use krad_host::HostLink;
use krad_store::{find_cloud_opencode_password, write_cloud_opencode_password};
use std::sync::Arc;

use super::super::chat::StartError;
use super::super::opencode::OpencodeLink;
use super::super::EnvironmentShell;
use super::CloudSite;

const NO_HOME: &str = "The cloud runner's home folder is not known yet";

fn runner_password(runner: CloudRunnerId) -> Result<OpencodePassword, String> {
    let kept = find_cloud_opencode_password(runner).map_err(|error| error.to_string())?;
    if let Some((password, _)) = kept {
        return Ok(password);
    }
    let password = new_opencode_password();
    write_cloud_opencode_password(runner, &password).map_err(|error| error.to_string())?;
    Ok(password)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn cloud_opencode_for(
        &mut self,
        site: &CloudSite,
        cx: &mut Context<Self>,
    ) -> Result<Arc<OpencodeServer>, StartError> {
        match self.cloud.opencode.get(&site.placement.runner) {
            Some(OpencodeLink::Ready { server, .. }) => Ok(Arc::clone(server)),
            Some(OpencodeLink::Failed(error)) => Err(StartError::Opencode(error.clone())),
            Some(OpencodeLink::Absent | OpencodeLink::Starting) => Err(StartError::Waiting),
            None => {
                self.start_cloud_opencode(site.clone(), cx)?;
                Err(StartError::Waiting)
            }
        }
    }

    pub(in crate::environment_shell) fn cloud_opencode_server(
        &self,
        runner: CloudRunnerId,
    ) -> Option<&Arc<OpencodeServer>> {
        match self.cloud.opencode.get(&runner)? {
            OpencodeLink::Ready { server, .. } => Some(server),
            OpencodeLink::Absent | OpencodeLink::Starting | OpencodeLink::Failed(_) => None,
        }
    }

    pub(in crate::environment_shell) fn ensure_cloud_opencode(
        &mut self,
        site: &CloudSite,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.cloud_opencode_for(site, cx) {
            tracing::debug!(%error, "opencode on the cloud runner is not ready");
        }
    }

    fn start_cloud_opencode(
        &mut self,
        site: CloudSite,
        cx: &mut Context<Self>,
    ) -> Result<(), StartError> {
        let program = self.launch_program(AgentPreset::Opencode)?;
        let HostLink::Remote(ssh) = &site.link else {
            return Err(StartError::Cloud(NO_HOME.to_owned()));
        };
        let home = ssh
            .home()
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| StartError::Cloud(NO_HOME.to_owned()))?;
        let runner = site.placement.runner;
        self.cloud.opencode.insert(runner, OpencodeLink::Starting);
        let started = cx.background_spawn(async move {
            let password = runner_password(runner)?;
            let run = CloudRun::new(
                site.link,
                site.cli,
                site.pairing,
                program,
                opencode_cloud_process(runner),
                JournalOffset::default(),
            );
            let server = OpencodeServer::start_cloud(&run, &home, &[], &password)
                .map_err(|error| error.to_string())?;
            let models = opencode_models(&server, None).map_err(|error| error.to_string())?;
            Ok::<_, String>((server, models))
        });
        cx.spawn(async move |this, cx| {
            let started = started.await;
            this.update(cx, |shell, cx| {
                let link = match started {
                    Ok((server, models)) => OpencodeLink::Ready { server, models },
                    Err(error) => {
                        tracing::warn!(%runner, %error, "opencode did not start on the cloud runner");
                        OpencodeLink::Failed(error)
                    }
                };
                shell.cloud.opencode.insert(runner, link);
                shell.load_waiting_histories(cx);
                shell.flush_chats_of(AgentPreset::Opencode, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        Ok(())
    }
}
