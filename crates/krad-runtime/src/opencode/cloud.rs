use krad_chat::{
    new_opencode_password, opencode_cloud_process, opencode_models, CloudRun, OpencodeServer,
};
use krad_domain::{CloudRunnerId, JournalOffset, OpencodePassword};
use krad_host::HostLink;
use krad_store::Secrets;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::Reach;
use crate::cloud_runs::CloudSite;
use crate::jobs::{JobDone, Jobs};

const NO_HOME: &str = "The cloud runner's home folder is not known yet";
const JOB_FAILED: &str = "opencode could not start on the cloud runner";

#[derive(Debug)]
enum CloudServed {
    Starting,
    Ready(Arc<OpencodeServer>),
    Failed(String),
}

#[derive(Debug)]
pub(crate) struct CloudOpencodeStarted {
    pub(crate) runner: CloudRunnerId,
    started: Result<Arc<OpencodeServer>, String>,
}

#[derive(Debug, Default)]
pub(crate) struct CloudOpencodes(HashMap<CloudRunnerId, CloudServed>);

fn runner_password(secrets: &Secrets, runner: CloudRunnerId) -> Result<OpencodePassword, String> {
    let kept = secrets
        .find_cloud_opencode_password(runner)
        .map_err(|error| error.to_string())?;
    if let Some((password, _)) = kept {
        return Ok(password);
    }
    let password = new_opencode_password();
    secrets
        .write_cloud_opencode_password(runner, &password)
        .map_err(|error| error.to_string())?;
    Ok(password)
}

fn start_on(
    site: &CloudSite,
    program: String,
    secrets: &Secrets,
) -> Result<Arc<OpencodeServer>, String> {
    let HostLink::Remote(ssh) = &site.link else {
        return Err(NO_HOME.to_owned());
    };
    let home = ssh.home().map(Path::to_path_buf).ok_or(NO_HOME)?;
    let runner = site.placement.runner;
    let password = runner_password(secrets, runner)?;
    let run = CloudRun::new(
        site.link.clone(),
        site.cli.clone(),
        site.pairing.clone(),
        program,
        opencode_cloud_process(runner),
        JournalOffset::default(),
    );
    let server = OpencodeServer::start_cloud(&run, &home, &[], &password)
        .map_err(|error| error.to_string())?;
    opencode_models(&server, None).map_err(|error| error.to_string())?;
    Ok(server)
}

impl CloudOpencodes {
    #[tracing::instrument(skip_all, fields(runner = %site.placement.runner))]
    pub(crate) fn reach(
        &mut self,
        site: &CloudSite,
        program: &str,
        (jobs, secrets): (&Jobs, &Secrets),
    ) -> Reach {
        let runner = site.placement.runner;
        match self.0.get(&runner) {
            Some(CloudServed::Ready(server)) => return Reach::Ready(Arc::clone(server)),
            Some(CloudServed::Failed(error)) => return Reach::Failed(error.clone()),
            Some(CloudServed::Starting) => return Reach::Waiting,
            None => {}
        }
        let (site, program, secrets) = (site.clone(), program.to_owned(), secrets.clone());
        let started = jobs.run(move || {
            let started = start_on(&site, program, &secrets);
            JobDone::CloudOpencodeStarted(CloudOpencodeStarted { runner, started })
        });
        let served = match started {
            Ok(()) => CloudServed::Starting,
            Err(error) => CloudServed::Failed(format!("{JOB_FAILED}: {error}")),
        };
        let reach = match &served {
            CloudServed::Failed(error) => Reach::Failed(error.clone()),
            CloudServed::Starting | CloudServed::Ready(_) => Reach::Waiting,
        };
        self.0.insert(runner, served);
        reach
    }

    pub(crate) fn started(&mut self, done: CloudOpencodeStarted) -> bool {
        if !matches!(self.0.get(&done.runner), Some(CloudServed::Starting)) {
            return false;
        }
        let served = match done.started {
            Ok(server) => {
                tracing::info!(runner = %done.runner, "opencode ready on the cloud runner");
                CloudServed::Ready(server)
            }
            Err(error) => {
                tracing::warn!(runner = %done.runner, %error, "opencode did not start on the cloud runner");
                CloudServed::Failed(error)
            }
        };
        self.0.insert(done.runner, served);
        true
    }

    pub(crate) fn forget(&mut self, runner: CloudRunnerId) {
        self.0.remove(&runner);
    }
}
