use krad_domain::{AutomationEvent, CodeHostTrigger, CodeHostWake};
use krad_gitlab::GitlabConfig;
use std::time::Instant;

use super::Runtime;
use crate::code_hosts::{observe, Observed, WatchTarget};
use crate::jobs::JobDone;

impl Runtime {
    pub(super) fn tick_code_hosts(&mut self) {
        let wanted = self.code_hosts.asked
            || self.events.wants_code_host()
            || self.kraddies.want_code_host();
        if !self.code_hosts.due(Instant::now(), wanted) {
            return;
        }
        let targets: Vec<WatchTarget> = self
            .environments
            .iter()
            .flat_map(|data| {
                let link = self.hosts.link(&data.host);
                data.groups.iter().map(move |group| WatchTarget {
                    environment: data.name.clone(),
                    group: group.id,
                    root: group.root.clone(),
                    link: link.clone(),
                })
            })
            .collect();
        let gitlab = GitlabConfig::try_from(&self.settings.current.gitlab).ok();
        let started = self
            .jobs
            .run(move || JobDone::CodeHosts(observe(targets, gitlab.as_ref())));
        if let Err(error) = started {
            tracing::warn!(%error, "could not start reading the code hosts");
            self.code_hosts.not_observed();
        }
    }

    pub(super) fn code_hosts_observed(&mut self, observed: Vec<Observed>) {
        for heard in self.code_hosts.observed(observed) {
            if let Some(event) = CodeHostTrigger::of(&heard.event) {
                let wake = CodeHostWake {
                    host: heard.host,
                    event,
                };
                let event = AutomationEvent::CodeHost(wake);
                self.wake_on(&heard.environment, Some(heard.group), event);
            }
            self.events.publish_own(
                &heard.environment,
                Some(heard.group),
                &heard.event,
                &mut self.outbox,
            );
        }
    }
}
