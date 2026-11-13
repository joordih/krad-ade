use krad_domain::{CloudProcessId, CloudProcessRun};
use krad_host::runner::{parse_status, run_authorized, status_command, CloudProcessStatus};
use krad_host::HostError;
use krad_protocol::AgentRef;
use std::process::Output;

use super::CloudSite;
use crate::jobs::{JobDone, Jobs};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Liveness {
    Checking,
    Running,
    Gone,
}

#[derive(Debug)]
pub(crate) struct LivenessChecked {
    pub(crate) node: AgentRef,
    pub(super) liveness: Liveness,
}

fn liveness_in(statuses: &[CloudProcessStatus], process: &CloudProcessId) -> Liveness {
    match statuses.iter().find(|status| status.id == *process) {
        Some(status) if status.run == CloudProcessRun::Running => Liveness::Running,
        Some(_) | None => Liveness::Gone,
    }
}

fn listing_of(output: Result<Output, HostError>) -> Result<String, String> {
    let output = output.map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn liveness_from(listing: Result<String, String>, process: &CloudProcessId) -> Liveness {
    let listed = listing.and_then(|text| parse_status(&text).map_err(|error| error.to_string()));
    match listed {
        Ok(statuses) => liveness_in(&statuses, process),
        Err(error) => {
            tracing::debug!(%process, %error, "the runner's processes could not be listed; attaching again");
            Liveness::Running
        }
    }
}

#[tracing::instrument(skip_all, fields(%process))]
pub(super) fn check(node: AgentRef, site: &CloudSite, process: CloudProcessId, jobs: &Jobs) {
    let (link, cli, pairing) = (site.link.clone(), site.cli.clone(), site.pairing.clone());
    let started = jobs.run(move || {
        let status = status_command(&cli, &pairing);
        let liveness = liveness_from(listing_of(run_authorized(&link, &status, &pairing)), &process);
        JobDone::CloudLiveness(LivenessChecked { node, liveness })
    });
    if let Err(error) = started {
        tracing::warn!(%error, "the cloud process check could not start");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn listed(text: &str) -> Result<String, String> {
        parse_status(text)
            .map(|_| text.to_owned())
            .map_err(|error| error.to_string())
    }

    #[test]
    fn only_a_running_process_is_attached_again() -> TestResult {
        let process: CloudProcessId = "api-a1-7-f00dbe".parse()?;
        let running = listed("api-a1-7-f00dbe\trunning\t120\n");
        assert_eq!(liveness_from(running, &process), Liveness::Running);
        let ended = listed("api-a1-7-f00dbe\tended:0\t120\n");
        assert_eq!(liveness_from(ended, &process), Liveness::Gone);
        let unknown = listed("other-1\trunning\t3\n");
        assert_eq!(liveness_from(unknown, &process), Liveness::Gone);
        Ok(())
    }

    #[test]
    fn an_unreadable_listing_never_starts_a_second_process() -> TestResult {
        let process: CloudProcessId = "api-a1-7-f00dbe".parse()?;
        let failed = listing_of(Err(HostError::NotConnected));
        assert_eq!(liveness_from(failed, &process), Liveness::Running);
        assert_eq!(
            liveness_from(listed("garbage"), &process),
            Liveness::Running
        );
        Ok(())
    }
}
