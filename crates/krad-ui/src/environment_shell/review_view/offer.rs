use gpui_kit::Context;
use krad_domain::{CodeHost, EnvironmentName, EnvironmentView, Host};
use krad_protocol::ReviewAct;
use std::path::PathBuf;

use crate::environment_shell::groups::GitKey;
use crate::environment_shell::EnvironmentShell;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Offer {
    Probing,
    On(CodeHost),
    Withheld,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn probe_review(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.active_group().map(|group| group.root.clone()) else {
            return;
        };
        let key = (self.active_host(), root);
        match self.review.offers.get(&key) {
            Some(Offer::Withheld) => self.leave_review(cx),
            Some(Offer::Probing | Offer::On(_)) => {}
            None => self.start_probe(key, cx),
        }
    }

    pub(in crate::environment_shell) fn review_host(&self) -> Option<CodeHost> {
        let root = self.active_group().map(|group| group.root.clone())?;
        match self.review.offers.get(&(self.active_host(), root)) {
            Some(Offer::On(host)) => Some(*host),
            Some(Offer::Probing | Offer::Withheld) | None => None,
        }
    }

    fn start_probe(&mut self, key: GitKey, cx: &mut Context<Self>) {
        self.review.offers.insert(key.clone(), Offer::Probing);
        self.post_review(key.1, ReviewAct::Probe, cx);
    }

    pub(super) fn review_probed(
        &mut self,
        environment: &EnvironmentName,
        root: PathBuf,
        host: Option<CodeHost>,
    ) {
        let Some(live) = self.envs.get(environment) else {
            return;
        };
        let key = (live.data.host.clone(), root);
        if self.review.offers.get(&key) != Some(&Offer::Probing) {
            return;
        }
        self.review
            .offers
            .insert(key, host.map_or(Offer::Withheld, Offer::On));
    }

    pub(in crate::environment_shell) fn forget_review_offers(
        &mut self,
        host: &Host,
        roots: &[PathBuf],
    ) {
        for root in roots {
            self.review.offers.remove(&(host.clone(), root.clone()));
        }
    }

    fn leave_review(&mut self, cx: &mut Context<Self>) {
        if self.active_view() != EnvironmentView::Review {
            return;
        }
        self.set_view(EnvironmentView::Sessions, cx);
    }
}
