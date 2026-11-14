use krad_chat::Runner;
use krad_domain::{AcpAgent, AcpAgentId, AgentPreset, ManagedAdapter};
use krad_protocol::AcpListing;
use std::path::PathBuf;

use super::harness_spots::Spotted;
use super::Runtime;
use crate::adapters::kept_package;
use crate::harness_sites::{HarnessHere, NotHere, NO_PROGRAM_TEXT};
use crate::models::AcpListingJob;

const NO_NODE_TEXT: &str =
    "needs Node.js to install itself. Install Node.js (with npm), then send the message again.";
const FAILED_TEXT: &str = "could not install itself:";

fn failed(error: String) -> AcpListing {
    AcpListing::Failed { error }
}

impl Runtime {
    pub(super) fn list_acp_offers(&mut self, spotted: &Spotted, agent: AcpAgentId) {
        let host = spotted.link.host();
        let Some(found) = self.settings.current.acp.agent(agent) else {
            return;
        };
        match self.acp_listing_runner(spotted, &found) {
            Ok((runner, cwd)) => {
                let job = AcpListingJob {
                    host,
                    agent: found,
                    runner,
                    cwd,
                    secrets: self.secrets.clone(),
                };
                self.models.list_acp(job, &self.jobs, &mut self.outbox);
            }
            Err(listing) => self
                .models
                .acp_unready((host, agent), listing, &mut self.outbox),
        }
    }

    fn acp_listing_runner(
        &mut self,
        spotted: &Spotted,
        agent: &AcpAgent,
    ) -> Result<(Runner, PathBuf), AcpListing> {
        let preset = AgentPreset::Acp(agent.id);
        let name = self.settings.current.acp.harness_name(preset);
        if kept_package(agent.id, &spotted.link, spotted.root.as_deref()).is_none() {
            let here = HarnessHere {
                link: &spotted.link,
                root: spotted.root.as_deref(),
                host_agents: self.host_agents(spotted),
            };
            return here
                .runner(preset, agent.program.clone())
                .map_err(|missing| match missing {
                    NotHere::Waiting => AcpListing::Waiting,
                    NotHere::NoProgram => failed(format!("{name} {NO_PROGRAM_TEXT}")),
                });
        }
        match self.adapters.state(agent.id) {
            Some(ManagedAdapter::Ready { program, .. }) => {
                let cwd = spotted
                    .root
                    .clone()
                    .or_else(|| spotted.home())
                    .unwrap_or_default();
                Ok((Runner::Native(program.clone()), cwd))
            }
            Some(ManagedAdapter::NoNode) => Err(failed(format!("{name} {NO_NODE_TEXT}"))),
            Some(ManagedAdapter::Failed(reason)) => {
                Err(failed(format!("{name} {FAILED_TEXT} {reason}")))
            }
            Some(ManagedAdapter::Checking | ManagedAdapter::Installing) => Err(AcpListing::Waiting),
            Some(ManagedAdapter::Unchecked) | None => {
                self.adapters.check(agent, &self.jobs, &mut self.outbox);
                Err(AcpListing::Waiting)
            }
        }
    }
}
