use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use ureq::Agent;

use crate::archive::place;
use crate::error::ExtensionError;
use crate::http::{download, latest_release};
use crate::platform::Platform;

const REPOSITORY: &str = "mason-org/mason-registry";
const ARCHIVE: &str = "registry.json.zip";
const FILE: &str = "registry.json";
const FOLDER: &str = "registry";
const DOWNLOAD_FILE: &str = "registry.download";
const PURL_SCHEME: &str = "pkg:";
const QUALIFIERS: char = '?';
const QUALIFIER_SEPARATOR: char = '&';
const VERSION_MARK: char = '@';
const PERCENT: u8 = b'%';
const HEX_RADIX: u32 = 16;
const ASSET_KEY: &str = "asset";
const DOWNLOAD_KEY: &str = "download";
const TARGET_KEY: &str = "target";
const MAX_AGE: Duration = Duration::from_hours(24);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Purl {
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) qualifiers: BTreeMap<String, String>,
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let pair = (byte == PERCENT)
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|pair| u8::from_str_radix(pair, HEX_RADIX).ok());
        if let Some(decoded) = pair {
            out.push(decoded);
            index += 3;
        } else {
            out.push(byte);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Purl {
    pub(crate) fn parse(id: &str) -> Option<Self> {
        let rest = id.strip_prefix(PURL_SCHEME)?;
        let (rest, qualifiers) = rest.split_once(QUALIFIERS).unwrap_or((rest, ""));
        let (kind, path) = rest.split_once('/')?;
        let (name, version) = path.split_once(VERSION_MARK)?;
        let qualifiers = qualifiers
            .split(QUALIFIER_SEPARATOR)
            .filter_map(|pair| pair.split_once('='))
            .map(|(key, value)| (key.to_owned(), decode(value)))
            .collect();
        Some(Self {
            kind: kind.to_owned(),
            name: decode(name),
            version: decode(version),
            qualifiers,
        })
    }
}

#[derive(Deserialize)]
struct RawSource {
    id: String,
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct RawPackage {
    name: String,
    #[serde(default)]
    languages: Vec<String>,
    source: RawSource,
    #[serde(default)]
    bin: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Asset,
    Download,
}

impl EntryKind {
    const fn key(self) -> &'static str {
        match self {
            Self::Asset => ASSET_KEY,
            Self::Download => DOWNLOAD_KEY,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) kind: EntryKind,
    pub(crate) targets: Vec<String>,
    pub(crate) fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub languages: Vec<String>,
    pub(crate) purl: Purl,
    pub(crate) entries: Vec<Entry>,
    pub(crate) bins: BTreeMap<String, String>,
}

fn targets_of(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(target)) => vec![target.clone()],
        Some(Value::Array(targets)) => targets
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn flatten(prefix: &str, value: &Value, out: &mut BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            out.insert(prefix.to_owned(), text.clone());
        }
        Value::Object(fields) => {
            for (key, inner) in fields {
                flatten(&format!("{prefix}.{key}"), inner, out);
            }
        }
        _ => {}
    }
}

fn entries(kind: EntryKind, value: Option<&Value>) -> Vec<Entry> {
    let items = match value {
        Some(Value::Array(items)) => items.clone(),
        Some(item @ Value::Object(_)) => vec![item.clone()],
        _ => Vec::new(),
    };
    items
        .iter()
        .filter_map(Value::as_object)
        .map(|item| {
            let mut fields = BTreeMap::new();
            for (key, value) in item {
                if key != TARGET_KEY {
                    flatten(key, value, &mut fields);
                }
            }
            Entry {
                targets: targets_of(item.get(TARGET_KEY)),
                kind,
                fields,
            }
        })
        .collect()
}

impl Package {
    fn from_raw(raw: RawPackage) -> Option<Self> {
        let purl = Purl::parse(&raw.source.id)?;
        let mut found = entries(EntryKind::Asset, raw.source.rest.get(ASSET_KEY));
        found.extend(entries(
            EntryKind::Download,
            raw.source.rest.get(DOWNLOAD_KEY),
        ));
        Some(Self {
            name: raw.name,
            languages: raw.languages,
            purl,
            entries: found,
            bins: raw.bin,
        })
    }

    pub(crate) fn entry_for(&self, platform: Platform) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|entry| entry.targets.is_empty() || platform.matches(&entry.targets))
    }

    pub(crate) fn entry_prefix(entry: &Entry) -> String {
        format!("source.{}", entry.kind.key())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    packages: BTreeMap<String, Package>,
}

impl Registry {
    pub(crate) fn from_json(text: &str) -> Result<Self, ExtensionError> {
        let raw: Vec<RawPackage> = serde_json::from_str(text)?;
        let packages = raw
            .into_iter()
            .filter_map(Package::from_raw)
            .map(|package| (package.name.clone(), package))
            .collect();
        Ok(Self { packages })
    }

    #[must_use]
    pub fn package(&self, name: &str) -> Option<&Package> {
        self.packages.get(name)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}

fn cache_file(root: &Path) -> PathBuf {
    root.join(FOLDER).join(FILE)
}

pub(crate) fn cached(root: &Path) -> Registry {
    fs::read_to_string(cache_file(root))
        .ok()
        .and_then(|text| Registry::from_json(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn is_stale(root: &Path) -> bool {
    fs::metadata(cache_file(root))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_none_or(|age| age > MAX_AGE)
}

pub(crate) fn refresh(
    agent: &Agent,
    root: &Path,
    platform: Platform,
) -> Result<Registry, ExtensionError> {
    let release = latest_release(agent, REPOSITORY)?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == ARCHIVE)
        .ok_or(ExtensionError::NoBuild)?;
    let folder = root.join(FOLDER);
    fs::create_dir_all(&folder)?;
    let archive = folder.join(DOWNLOAD_FILE);
    download(
        agent,
        &asset.browser_download_url,
        asset.sha256().as_ref(),
        &archive,
    )?;
    place(&archive, ARCHIVE, &folder, platform.os)?;
    fs::remove_file(&archive)?;
    let text = fs::read_to_string(cache_file(root))?;
    Registry::from_json(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_urls_split_into_kind_name_version_and_qualifiers() {
        let npm = Purl::parse("pkg:npm/%40vtsls/language-server@0.3.0");
        assert_eq!(
            npm.as_ref().map(|purl| purl.name.as_str()),
            Some("@vtsls/language-server")
        );
        let generic = Purl::parse("pkg:generic/Kotlin/kotlin-lsp@kotlin-lsp/v263.7084.0");
        assert_eq!(
            generic.as_ref().map(|purl| purl.version.as_str()),
            Some("kotlin-lsp/v263.7084.0")
        );
        let pypi = Purl::parse("pkg:pypi/python-lsp-server@1.15.0?extra=all");
        assert_eq!(
            pypi.as_ref()
                .and_then(|purl| purl.qualifiers.get("extra").cloned()),
            Some("all".to_owned())
        );
        assert!(Purl::parse("github/x").is_none());
    }

    #[test]
    fn a_package_picks_the_asset_of_its_platform() -> Result<(), ExtensionError> {
        let json = r#"[{"name":"taplo","languages":["TOML"],
            "source":{"id":"pkg:github/tamasfe/taplo@0.10.0","asset":[
              {"target":"darwin_arm64","file":"taplo-darwin-aarch64.gz","bin":"taplo-darwin-aarch64"},
              {"target":["linux_x64_gnu","linux_x64"],"file":"taplo-linux-x86_64.gz","bin":"taplo"}]},
            "bin":{"taplo":"{{source.asset.bin}}"}}]"#;
        let registry = Registry::from_json(json)?;
        let taplo = registry.package("taplo").ok_or(ExtensionError::NoBuild)?;
        let mac = Platform::current().filter(|platform| platform.targets()[0] == "darwin_arm64");
        if let Some(mac) = mac {
            let entry = taplo.entry_for(mac).ok_or(ExtensionError::NoBuild)?;
            assert_eq!(
                entry.fields.get("file").map(String::as_str),
                Some("taplo-darwin-aarch64.gz")
            );
        }
        assert_eq!(taplo.purl.kind, "github");
        Ok(())
    }
}
