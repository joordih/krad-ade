use krad_chat::{acp_offers, Runner};
use krad_domain::{AcpAgent, AcpAgentId, AcpAgentStart, AcpOffered, AgentPreset, Host};
use krad_protocol::{AcpListing, AcpOffers, EventKind, ModelsOrigin};
use std::path::PathBuf;

use super::Models;
use crate::jobs::{JobDone, Jobs};

const LISTING_NOT_STARTED: &str = "the model listing could not start";

#[derive(Debug)]
pub(crate) struct AcpOffersRead {
    host: Host,
    agent: AcpAgentId,
    read: Result<AcpOffered, String>,
}

pub(crate) struct AcpListingJob {
    pub(crate) host: Host,
    pub(crate) agent: AcpAgent,
    pub(crate) runner: Runner,
    pub(crate) cwd: PathBuf,
    pub(crate) secrets: krad_store::Secrets,
}

fn offers(host: Host, agent: AcpAgentId, listing: AcpListing) -> EventKind {
    EventKind::AcpOffers(AcpOffers {
        host,
        agent,
        listing,
    })
}

impl Models {
    pub(crate) fn acp_unready(
        &self,
        (host, agent): (Host, AcpAgentId),
        listing: AcpListing,
        outbox: &mut Vec<EventKind>,
    ) {
        if self.acp_listing.contains(&(host.clone(), agent)) {
            return;
        }
        outbox.push(offers(host, agent, listing));
    }

    pub(crate) fn list_acp(
        &mut self,
        job: AcpListingJob,
        jobs: &Jobs,
        outbox: &mut Vec<EventKind>,
    ) {
        let key = (job.host.clone(), job.agent.id);
        if !self.acp_listing.insert(key.clone()) {
            return;
        }
        let started = jobs.run(move || {
            let AcpListingJob {
                host,
                agent,
                runner,
                cwd,
                secrets,
            } = job;
            let id = agent.id;
            let read = secrets
                .read_acp_secrets(&agent)
                .map_err(|error| error.to_string())
                .and_then(|secrets| {
                    acp_offers(&runner, &cwd, AcpAgentStart { agent, secrets })
                        .map_err(|error| error.to_string())
                });
            JobDone::AcpOffersRead(AcpOffersRead {
                host,
                agent: id,
                read,
            })
        });
        let listing = match started {
            Ok(()) => AcpListing::Listing,
            Err(error) => {
                tracing::warn!(%error, "an ACP model listing could not start");
                self.acp_listing.remove(&key);
                AcpListing::Failed {
                    error: LISTING_NOT_STARTED.to_owned(),
                }
            }
        };
        outbox.push(offers(key.0, key.1, listing));
    }

    pub(crate) fn acp_listed(&mut self, read: AcpOffersRead, outbox: &mut Vec<EventKind>) {
        let AcpOffersRead { host, agent, read } = read;
        self.acp_listing.remove(&(host.clone(), agent));
        let listing = match read {
            Ok(offered) => {
                let key = (host.clone(), AgentPreset::Acp(agent));
                self.keep(key, offered.models.clone(), ModelsOrigin::Listing, outbox);
                AcpListing::Offered { offered }
            }
            Err(error) => AcpListing::Failed { error },
        };
        outbox.push(offers(host, agent, listing));
    }
}
