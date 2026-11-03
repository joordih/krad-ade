use krad_domain::{ServerRuntimeId, VersionConstraint};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use ureq::Agent;

use crate::http::fetch_text;
use crate::manifest::Manifests;
use crate::package::{tool, Tool};
use crate::registry::Purl;

pub(crate) type RuntimeRequirements = BTreeMap<ServerRuntimeId, VersionConstraint>;

const PYPI_KIND: &str = "pypi";
const NPM_KIND: &str = "npm";
const PYPI_URL: &str = "https://pypi.org/pypi";
const NPM_URL: &str = "https://registry.npmjs.org";
const PYPI_RELEASE_FILE: &str = "json";
const SCOPE_SEPARATOR: &str = "/";
const ENCODED_SCOPE_SEPARATOR: &str = "%2f";
const CACHE_FILE: &str = "runtime-requirements.json";
const KEY_SEPARATOR: char = ' ';

#[derive(Deserialize)]
struct PypiRelease {
    info: PypiInfo,
}

#[derive(Deserialize)]
struct PypiInfo {
    #[serde(default)]
    requires_python: Option<String>,
}

#[derive(Deserialize)]
struct NpmVersion {
    #[serde(default)]
    engines: Option<NpmEngines>,
}

#[derive(Deserialize)]
struct NpmEngines {
    #[serde(default)]
    node: Option<String>,
}

pub(crate) struct RequirementCache {
    file: PathBuf,
    known: Mutex<HashMap<String, Option<String>>>,
}

fn key(purl: &Purl) -> String {
    format!(
        "{}{KEY_SEPARATOR}{}{KEY_SEPARATOR}{}",
        purl.kind, purl.name, purl.version
    )
}

fn release_url(purl: &Purl) -> Option<String> {
    match purl.kind.as_str() {
        PYPI_KIND => Some(format!(
            "{PYPI_URL}/{}/{}/{PYPI_RELEASE_FILE}",
            purl.name, purl.version
        )),
        NPM_KIND => Some(format!(
            "{NPM_URL}/{}/{}",
            purl.name.replace(SCOPE_SEPARATOR, ENCODED_SCOPE_SEPARATOR),
            purl.version
        )),
        _ => None,
    }
}

pub(crate) fn declared_requirement(kind: &str, body: &str) -> Option<String> {
    let declared = match kind {
        PYPI_KIND => {
            serde_json::from_str::<PypiRelease>(body)
                .ok()?
                .info
                .requires_python
        }
        NPM_KIND => serde_json::from_str::<NpmVersion>(body).ok()?.engines?.node,
        _ => None,
    };
    declared.filter(|text| !text.trim().is_empty())
}

impl RequirementCache {
    pub(crate) fn load(root: &Path) -> Self {
        let file = root.join(CACHE_FILE);
        let known = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            file,
            known: Mutex::new(known),
        }
    }

    fn declared(&self, agent: &Agent, purl: &Purl) -> Option<String> {
        let key = key(purl);
        if let Some(known) = self
            .known
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return known.clone();
        }
        let url = release_url(purl)?;
        let body = match fetch_text(agent, &url) {
            Ok(body) => body,
            Err(error) => {
                tracing::debug!(%error, kind = purl.kind, "package metadata unavailable");
                return None;
            }
        };
        let declared = declared_requirement(&purl.kind, &body);
        let mut known = self.known.lock().unwrap_or_else(PoisonError::into_inner);
        known.insert(key, declared.clone());
        if let Ok(bytes) = serde_json::to_vec(&*known) {
            if let Err(error) = fs::write(&self.file, bytes) {
                tracing::debug!(%error, "runtime requirements cache not written");
            }
        }
        declared
    }

    pub(crate) fn requirements(
        &self,
        agent: &Agent,
        manifests: &Manifests,
        purl: &Purl,
    ) -> RuntimeRequirements {
        let mut requirements = RuntimeRequirements::new();
        let Some(Tool::Manager(server_runtime, _) | Tool::Runner(server_runtime, _)) =
            tool(manifests, &purl.kind)
        else {
            return requirements;
        };
        let Some(declared) = self.declared(agent, purl) else {
            return requirements;
        };
        match declared.parse::<VersionConstraint>() {
            Ok(constraint) => {
                requirements.insert(server_runtime.id.clone(), constraint);
            }
            Err(error) => tracing::debug!(%error, "package runtime requirement not understood"),
        }
        requirements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pypi_and_npm_metadata_name_their_runtime_range() {
        let pypi = r#"{"info":{"name":"basedpyright","requires_python":">=3.10"}}"#;
        assert_eq!(
            declared_requirement(PYPI_KIND, pypi).as_deref(),
            Some(">=3.10")
        );
        let npm = r#"{"name":"@vtsls/language-server","engines":{"node":">=16"}}"#;
        assert_eq!(declared_requirement(NPM_KIND, npm).as_deref(), Some(">=16"));
        assert_eq!(declared_requirement(NPM_KIND, r#"{"name":"x"}"#), None);
        assert_eq!(
            declared_requirement(PYPI_KIND, r#"{"info":{"requires_python":""}}"#),
            None
        );
    }

    #[test]
    fn scoped_npm_names_are_escaped_in_the_url() {
        let purl = Purl {
            kind: NPM_KIND.to_owned(),
            name: "@vtsls/language-server".to_owned(),
            version: "0.3.0".to_owned(),
            qualifiers: BTreeMap::new(),
        };
        assert_eq!(
            release_url(&purl).as_deref(),
            Some("https://registry.npmjs.org/@vtsls%2flanguage-server/0.3.0")
        );
    }
}
