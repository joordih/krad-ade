use super::*;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const OLD: &str = "0123456789abcdef0123456789abcdef01234567";
const NEW: &str = "89abcdef0123456789abcdef0123456789abcdef";

const INDEX: &str = r#"{
  "extensions": [
    {"id": "docker", "name": "Docker", "description": "Containers and images",
     "repository": "https://github.com/joordih/krad-docker", "commit": "89abcdef0123456789abcdef0123456789abcdef",
     "api": "1.0", "tags": ["containers", "compose"]},
    {"id": "dockerfile-lint", "name": "Hadolint", "description": "Lints a Docker file",
     "repository": "git@github.com:acme/krad-hadolint.git", "commit": "0123456789abcdef0123456789abcdef01234567",
     "api": "1.0"},
    {"id": "kube", "name": "Kubernetes", "description": "Pods, deployments and logs",
     "repository": "https://github.com/acme/krad-kube", "commit": "0123456789abcdef0123456789abcdef01234567",
     "api": "9.0", "tags": ["docker"]},
    {"id": "docker", "name": "Docker again", "repository": "https://github.com/x/y",
     "commit": "0123456789abcdef0123456789abcdef01234567", "api": "1.0"},
    {"id": "Bad Id", "name": "Bad", "repository": "https://github.com/x/y",
     "commit": "0123456789abcdef0123456789abcdef01234567", "api": "1.0"},
    {"id": "short", "name": "Short", "repository": "https://github.com/x/y",
     "commit": "abc", "api": "1.0"},
    {"id": "inject", "name": "Inject", "repository": "--upload-pack=touch /tmp/pwned",
     "commit": "0123456789abcdef0123456789abcdef01234567", "api": "1.0"}
  ]
}"#;

fn ids(entries: &[&IndexEntry]) -> Vec<String> {
    entries.iter().map(|entry| entry.id.to_string()).collect()
}

fn pinned(id: &str, commit: &str) -> Result<ExtensionEntry, krad_domain::GitCommitError> {
    Ok(ExtensionEntry {
        id: id.to_owned(),
        source: ExtensionSource::Pinned {
            url: "https://github.com/joordih/krad-docker".to_owned(),
            commit: commit.parse()?,
        },
        enabled: true,
        trusted: None,
    })
}

#[test]
fn valid_entries_are_kept_and_broken_ones_are_reported_by_position() -> TestResult {
    let index = ExtensionIndex::parse(INDEX)?;
    assert_eq!(
        index
            .entries
            .iter()
            .map(|entry| entry.id.to_string())
            .collect::<Vec<_>>(),
        ["docker", "dockerfile-lint", "kube"]
    );
    let problems: Vec<_> = index
        .problems
        .iter()
        .map(|problem| problem.position)
        .collect();
    assert_eq!(problems, [3, 4, 5, 6]);
    assert!(matches!(index.problems[0].error, EntryError::Duplicate(_)));
    assert!(matches!(index.problems[1].error, EntryError::Id(_)));
    assert!(matches!(index.problems[2].error, EntryError::Commit(_)));
    assert_eq!(index.problems[3].error, EntryError::Address);
    Ok(())
}

#[test]
fn a_file_that_is_not_an_index_is_refused() {
    assert!(ExtensionIndex::parse(r#"{"plugins": []}"#).is_err());
    assert!(ExtensionIndex::parse("[").is_err());
}

#[test]
fn search_ranks_exact_ids_then_prefixes_then_mentions() -> TestResult {
    let index = ExtensionIndex::parse(INDEX)?;
    assert_eq!(
        ids(&index.search("DOCKER")),
        ["docker", "dockerfile-lint", "kube"]
    );
    assert_eq!(ids(&index.search("pods")), ["kube"]);
    assert_eq!(ids(&index.search("compose")), ["docker"]);
    assert_eq!(index.search("  ").len(), 3);
    assert_eq!(index.search("nothing-like-this"), Vec::<&IndexEntry>::new());
    Ok(())
}

#[test]
fn entries_needing_a_newer_api_stay_listed_but_flagged() -> TestResult {
    let index = ExtensionIndex::parse(INDEX)?;
    let kube = index.get("kube").ok_or("kube is listed")?;
    assert_eq!(kube.compatibility(), Compatibility::NeedsNewerApp);
    let docker = index.get("docker").ok_or("docker is listed")?;
    assert_eq!(docker.compatibility(), Compatibility::Supported);
    Ok(())
}

#[test]
fn updates_list_pinned_extensions_behind_the_index_only() -> TestResult {
    let index = ExtensionIndex::parse(INDEX)?;
    let installed = [
        pinned("docker", OLD)?,
        pinned("dockerfile-lint", OLD)?,
        pinned("gone", OLD)?,
        ExtensionEntry {
            id: "kube".to_owned(),
            source: ExtensionSource::Git("https://github.com/acme/krad-kube".to_owned()),
            enabled: true,
            trusted: None,
        },
    ];
    let updates = index.updates(&installed);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].extension, "docker");
    assert_eq!(updates[0].available.as_ref(), NEW);
    Ok(())
}

#[test]
fn an_up_to_date_or_unpinned_extension_needs_no_update() -> TestResult {
    let index = ExtensionIndex::parse(INDEX)?;
    let config = PathBuf::from("/nonexistent-krad-config");
    assert_eq!(
        update_from_index(&config, &pinned("docker", NEW)?, &index)?,
        None
    );
    let folder = ExtensionEntry {
        id: "docker".to_owned(),
        source: ExtensionSource::Folder(Path::new("/x").to_path_buf()),
        enabled: true,
        trusted: None,
    };
    assert_eq!(update_from_index(&config, &folder, &index)?, None);
    Ok(())
}

#[test]
fn an_incompatible_entry_is_never_installed() -> TestResult {
    let index = ExtensionIndex::parse(INDEX)?;
    let kube = index.get("kube").ok_or("kube is listed")?;
    let config = std::env::temp_dir().join(format!("krad-index-install-{}", std::process::id()));
    assert!(matches!(
        add_from_index(&config, kube),
        Err(crate::ExtensionError::Incompatible { .. })
    ));
    Ok(())
}
