use krad_domain::{AppEvent, EnvironmentName, SessionId};
use krad_protocol::CloudNews;
use krad_repo::ORIGIN_REMOTE;

use super::Runtime;
use crate::cloud::{
    bring_home, open_request, push_branch, upstream_branch, upstream_remote, CloudDone, CloudWork,
    LocalRepo,
};
use crate::reviews::gitlab_config;

impl Runtime {
    fn cloud_work(
        &mut self,
        environment: &EnvironmentName,
        session: SessionId,
    ) -> Option<CloudWork> {
        let data = self.environments.data(environment)?;
        let Some(found) = data.session(session) else {
            self.cloud_news(CloudNews::NoSession);
            return None;
        };
        let Some(placement) = found.cloud.clone() else {
            self.cloud_news(CloudNews::NotCloud);
            return None;
        };
        let group = found.group;
        let name = found.name.clone();
        let upstream = group
            .and_then(|group| data.group(group))
            .and_then(|group| self.git.at(&data.host, &group.root))
            .and_then(|git| git.status.upstream.clone());
        let Some(link) = self
            .runners
            .directory
            .get(placement.runner)
            .map(|ready| ready.link)
        else {
            let runner = placement.runner;
            if let krad_protocol::Reply::Refused { message, .. } = self.ensure_runner(runner) {
                tracing::debug!(%runner, %message, "the cloud session's runner is unknown");
            }
            self.cloud_news(CloudNews::RunnerNotReady);
            return None;
        };
        Some(CloudWork {
            env: environment.clone(),
            group,
            name,
            placement,
            link,
            upstream,
        })
    }

    pub(super) fn push_cloud_branch(&mut self, environment: &EnvironmentName, session: SessionId) {
        let Some(work) = self.cloud_work(environment, session) else {
            return;
        };
        self.cloud_news(CloudNews::Pushing);
        self.run_cloud_job(move || CloudDone::Pushed {
            branch: work.placement.branch.clone(),
            pushed: push_branch(&work.link, &work.placement),
        });
    }

    pub(super) fn open_cloud_request(&mut self, environment: &EnvironmentName, session: SessionId) {
        let Some(mut work) = self.cloud_work(environment, session) else {
            return;
        };
        work.upstream = upstream_branch(work.upstream.as_deref()).map(str::to_owned);
        self.cloud_news(CloudNews::Opening);
        let gitlab = gitlab_config(&self.settings.current.gitlab).ok();
        let secrets = self.secrets.clone();
        self.run_cloud_job(move || CloudDone::RequestOpened {
            opened: open_request(&work, gitlab, &secrets).map(Box::new),
            environment: work.env,
            group: work.group,
        });
    }

    pub(super) fn bring_cloud_home(&mut self, environment: &EnvironmentName, session: SessionId) {
        let Some(work) = self.cloud_work(environment, session) else {
            return;
        };
        let Some(data) = self.environments.data(environment) else {
            return;
        };
        let host = data.host.clone();
        let root = work
            .group
            .and_then(|group| data.group(group))
            .and_then(|group| self.git.at(&host, &group.root))
            .map(|git| git.repo_root.clone());
        let Some(root) = root else {
            return self.cloud_news(CloudNews::NoRepo);
        };
        let local = LocalRepo {
            link: self.hosts.link(&host),
            remote: upstream_remote(work.upstream.as_deref())
                .unwrap_or(ORIGIN_REMOTE)
                .to_owned(),
            askpass: self.git_askpass(environment, &host, &root),
            root,
        };
        self.cloud_news(CloudNews::Bringing);
        self.run_cloud_job(move || CloudDone::BroughtHome {
            environment: work.env.clone(),
            repo_root: local.root.clone(),
            branch: work.placement.branch.clone(),
            brought: bring_home(&work, local),
        });
    }

    pub(super) fn cloud_result(&mut self, done: CloudDone) {
        let news = match done {
            CloudDone::Pushed { branch, pushed } => match pushed {
                Ok(()) => CloudNews::Pushed { branch },
                Err(error) => CloudNews::ResultFailed {
                    error: error.to_string(),
                },
            },
            CloudDone::RequestOpened {
                environment,
                group,
                opened,
            } => match opened {
                Ok(request) => {
                    let event = AppEvent::MergeRequestOpened {
                        merge_request: request.iid,
                    };
                    self.events
                        .publish_own(&environment, group, &event, &mut self.outbox);
                    CloudNews::RequestOpened {
                        url: request.web_url,
                    }
                }
                Err(error) => {
                    tracing::info!(%error, "no request was opened for the cloud session");
                    CloudNews::ResultFailed {
                        error: error.to_string(),
                    }
                }
            },
            CloudDone::BroughtHome {
                environment,
                repo_root,
                branch,
                brought,
            } => match brought {
                Ok(worktree) => {
                    self.open_worktree_group(&environment, repo_root, worktree);
                    CloudNews::BroughtHome { branch }
                }
                Err(error) => CloudNews::ResultFailed {
                    error: error.to_string(),
                },
            },
            CloudDone::RemoteRead { .. } | CloudDone::Prepared { .. } => return,
        };
        self.cloud_news(news);
    }
}
