use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

pub const ORIGIN_REMOTE: &str = "origin";
const REMOTE_SEPARATOR: char = '/';

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "place", content = "remote", rename_all = "snake_case")]
pub enum BranchPlace {
    Local,
    Remote(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Branch {
    pub name: String,
    pub place: BranchPlace,
    pub upstream: Option<String>,
    pub is_head: bool,
}

impl fmt::Display for Branch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.place {
            BranchPlace::Local => f.write_str(&self.name),
            BranchPlace::Remote(remote) => write!(f, "{remote}{REMOTE_SEPARATOR}{}", self.name),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Refs {
    pub branches: Vec<Branch>,
    pub tags: Vec<String>,
}

impl Refs {
    pub fn local(&self) -> impl Iterator<Item = &Branch> {
        self.branches
            .iter()
            .filter(|branch| branch.place == BranchPlace::Local)
    }

    pub fn remote(&self) -> impl Iterator<Item = &Branch> {
        self.branches
            .iter()
            .filter(|branch| matches!(branch.place, BranchPlace::Remote(_)))
    }

    #[must_use]
    pub fn preferred_remote(&self) -> String {
        let remotes: BTreeSet<&str> = self
            .remote()
            .filter_map(|branch| match &branch.place {
                BranchPlace::Remote(remote) => Some(remote.as_str()),
                BranchPlace::Local => None,
            })
            .collect();
        if remotes.is_empty() || remotes.contains(ORIGIN_REMOTE) {
            return ORIGIN_REMOTE.to_owned();
        }
        remotes
            .into_iter()
            .next()
            .map_or_else(|| ORIGIN_REMOTE.to_owned(), str::to_owned)
    }
}
