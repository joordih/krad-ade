use krad_domain::review::JobId;
use krad_domain::review::MergeRequestIid;
use krad_domain::{AgentPreset, CodeHost, EnvironmentName, ReviewVerdict};
use krad_protocol::{RequestId, ReviewAnswer, ReviewConnection};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::code_hosts::HostClient;

mod fix;
mod logins;
mod owner;
mod reads;

pub(crate) use fix::{read_fix, FixRead};
pub(crate) use logins::{
    check as check_login, gitlab_config, keep as keep_login, sign_in, sign_out, LoginDone, Logins,
};
pub(crate) use owner::{branch_owner, pick_branch, shares_repository, ChatSeat, FixOwner};
pub(crate) use reads::{connect, detail, facts, issues, list, probe, write};

pub(crate) type ReviewKey = (EnvironmentName, PathBuf);

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ReviewReply {
    pub(crate) request: RequestId,
    pub(crate) environment: EnvironmentName,
    pub(crate) root: PathBuf,
}

impl ReviewReply {
    pub(crate) fn key(&self) -> ReviewKey {
        (self.environment.clone(), self.root.clone())
    }
}

#[derive(Debug)]
pub(crate) enum ReviewDone {
    Answered {
        reply: ReviewReply,
        answer: ReviewAnswer,
    },
    Connected {
        reply: ReviewReply,
        client: Option<Arc<HostClient>>,
        connection: ReviewConnection,
    },
    FixRead {
        reply: ReviewReply,
        job: JobId,
        harness: AgentPreset,
        read: Result<FixRead, String>,
    },
    Login(LoginDone),
}

#[derive(Debug, Default)]
pub(crate) struct Reviews {
    connections: HashMap<ReviewKey, Arc<HostClient>>,
    pub(crate) verdicts: HashMap<RequestId, (MergeRequestIid, ReviewVerdict)>,
    pub(crate) logins: Logins,
}

impl Reviews {
    pub(crate) fn client(&self, key: &ReviewKey) -> Option<Arc<HostClient>> {
        self.connections.get(key).map(Arc::clone)
    }

    pub(crate) fn connected(&mut self, key: ReviewKey, client: Option<Arc<HostClient>>) {
        match client {
            Some(client) => {
                self.connections.insert(key, client);
            }
            None => {
                self.connections.remove(&key);
            }
        }
    }

    pub(crate) fn forget_host(&mut self, host: CodeHost) {
        self.connections
            .retain(|_, client| client.code_host() != host);
    }
}
