use gpui_kit::{AppContext as _, Context};
use krad_domain::{CodeHost, EnvironmentView, GitlabSettings};
use krad_gitlab::GitlabConfig;
use std::path::{Path, PathBuf};

use super::host::origin_repo;

use crate::environment_shell::EnvironmentShell;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Offer {
    Probing,
    On(CodeHost),
    Withheld,
}

fn probe(root: &Path, gitlab: &GitlabSettings) -> Option<CodeHost> {
    let repo = origin_repo(root, GitlabConfig::try_from(gitlab).ok())?;
    repo.login
        .client()
        .ok()
        .flatten()
        .map(|_| repo.login.code_host())
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn probe_review(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.active_group().map(|group| group.root.clone()) else {
            return;
        };
        match self.review.offers.get(&root) {
            Some(Offer::Withheld) => self.leave_review(cx),
            Some(Offer::Probing | Offer::On(_)) => {}
            None => self.start_probe(root, cx),
        }
    }

    pub(in crate::environment_shell) fn review_host(&self) -> Option<CodeHost> {
        let root = self.active_group().map(|group| group.root.as_path())?;
        match self.review.offers.get(root) {
            Some(Offer::On(host)) => Some(*host),
            Some(Offer::Probing | Offer::Withheld) | None => None,
        }
    }

    fn start_probe(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        self.review.offers.insert(root.clone(), Offer::Probing);
        let gitlab = self.settings.gitlab.clone();
        let probed = cx.background_spawn({
            let root = root.clone();
            async move { probe(&root, &gitlab) }
        });
        cx.spawn(async move |this, cx| {
            let host = probed.await;
            this.update(cx, |shell, cx| {
                let offer = host.map_or(Offer::Withheld, Offer::On);
                shell.review.offers.insert(root, offer);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn leave_review(&mut self, cx: &mut Context<Self>) {
        if self.active_view() != EnvironmentView::Review {
            return;
        }
        self.set_view(EnvironmentView::Sessions, cx);
    }
}
