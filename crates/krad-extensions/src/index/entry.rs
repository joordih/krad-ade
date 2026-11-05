use krad_domain::{GitCommit, GitCommitError};
use krad_extension_api::{ApiVersion, ApiVersionError, ExtensionId, IdError, CURRENT_API};
use serde::Deserialize;
use std::fmt;

const ADDRESS_PREFIXES: [&str; 3] = ["https://", "ssh://", "git@"];
const OPTION_MARKER: char = '-';
const SCP_SEPARATOR: char = ':';
const PATH_SEPARATOR: char = '/';

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitAddress(String);

impl GitAddress {
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let clean = !text.is_empty()
            && !text.starts_with(OPTION_MARKER)
            && !text.chars().any(|c| c.is_whitespace() || c.is_control());
        if !clean {
            return None;
        }
        let rest = ADDRESS_PREFIXES
            .iter()
            .find_map(|prefix| text.strip_prefix(prefix))?;
        let separator = if text.starts_with("git@") {
            SCP_SEPARATOR
        } else {
            PATH_SEPARATOR
        };
        let (host, path) = rest.split_once(separator)?;
        let has_path = !path.trim_matches(PATH_SEPARATOR).is_empty();
        (!host.is_empty() && has_path).then(|| Self(text.to_owned()))
    }
}

impl AsRef<str> for GitAddress {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GitAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    Supported,
    NeedsNewerApp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub id: ExtensionId,
    pub name: String,
    pub description: String,
    pub repository: GitAddress,
    pub commit: GitCommit,
    pub api: ApiVersion,
    pub tags: Vec<String>,
}

impl IndexEntry {
    #[must_use]
    pub fn compatibility(&self) -> Compatibility {
        if CURRENT_API.supports(self.api) {
            Compatibility::Supported
        } else {
            Compatibility::NeedsNewerApp
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EntryError {
    #[error("its id is invalid: {0}")]
    Id(#[from] IdError),
    #[error("its commit is invalid: {0}")]
    Commit(#[from] GitCommitError),
    #[error("its repository is not an https, ssh or git@ address")]
    Address,
    #[error("its API version is invalid: {0}")]
    Api(#[from] ApiVersionError),
    #[error("its name is empty")]
    Name,
    #[error("its id {0} is already listed")]
    Duplicate(ExtensionId),
}

#[derive(Debug, Deserialize)]
pub(super) struct RawEntry {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    repository: String,
    commit: String,
    api: String,
    #[serde(default)]
    tags: Vec<String>,
}

impl TryFrom<RawEntry> for IndexEntry {
    type Error = EntryError;

    fn try_from(raw: RawEntry) -> Result<Self, Self::Error> {
        let name = raw.name.trim().to_owned();
        if name.is_empty() {
            return Err(EntryError::Name);
        }
        Ok(Self {
            id: raw.id.trim().parse()?,
            name,
            description: raw.description.trim().to_owned(),
            repository: GitAddress::parse(&raw.repository).ok_or(EntryError::Address)?,
            commit: raw.commit.parse()?,
            api: raw.api.trim().parse()?,
            tags: raw
                .tags
                .into_iter()
                .map(|tag| tag.trim().to_owned())
                .filter(|tag| !tag.is_empty())
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_ssh_and_scp_addresses_with_a_path_are_accepted() {
        let accepted = [
            "https://github.com/joordih/krad-docker",
            "https://github.com/joordih/krad-docker.git",
            "ssh://git@github.com/joordih/krad-docker.git",
            "git@github.com:joordih/krad-docker.git",
        ];
        for address in accepted {
            assert!(GitAddress::parse(address).is_some(), "{address}");
        }
        let refused = [
            "",
            "--upload-pack=touch /tmp/x",
            "http://github.com/joordih/krad-docker",
            "file:///etc",
            "https://github.com/",
            "git@github.com",
            "https://github.com/a b",
            "ext::sh -c touch",
        ];
        for address in refused {
            assert!(GitAddress::parse(address).is_none(), "{address}");
        }
    }
}
