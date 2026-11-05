use krad_domain::{ExtensionEntry, ExtensionSource, IndexRepository};
use krad_extensions::index::{Compatibility, ExtensionIndex, IndexEntry, IndexOrigin};
use std::collections::BTreeSet;
use std::time::Duration;

const MINUTE: Duration = Duration::from_mins(1);
const HOUR: Duration = Duration::from_hours(1);
const DAY: Duration = Duration::from_hours(24);
const TAG_SEPARATOR: &str = " · ";
const MINUTES_UNIT: &str = "min";
const HOURS_UNIT: &str = "h";
const DAYS_UNIT: &str = "d";
const AGO: &str = "ago";
const JUST_NOW: &str = "just now";
const UPDATED_PREFIX: &str = "Updated";
const OFFLINE_PREFIX: &str = "Offline, showing the index from";
const ONE_PROBLEM: &str = "1 entry of the index could not be read";
const PROBLEMS_SUFFIX: &str = "entries of the index could not be read";
const MISSING_PREFIX: &str = "There is no extension index at";
const MISSING_SUFFIX: &str = "yet";
const UNREACHABLE_PREFIX: &str = "The extension index at";
const UNREACHABLE_SUFFIX: &str = "could not be loaded";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::environment_shell) enum LoadFailure {
    Missing,
    Unreachable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowAction {
    Install,
    Installed,
    Update,
    Working,
    NeedsNewerApp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IndexRow {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) detail: String,
    pub(super) action: RowAction,
}

fn action_for(
    entry: &IndexEntry,
    installed: &[ExtensionEntry],
    working: &BTreeSet<String>,
) -> RowAction {
    let id = entry.id.to_string();
    if working.contains(&id) {
        return RowAction::Working;
    }
    let compatible = entry.compatibility() == Compatibility::Supported;
    match installed.iter().find(|known| known.id.as_ref() == id) {
        Some(known) => match &known.source {
            ExtensionSource::Pinned { commit, .. } if *commit != entry.commit && compatible => {
                RowAction::Update
            }
            _ => RowAction::Installed,
        },
        None if compatible => RowAction::Install,
        None => RowAction::NeedsNewerApp,
    }
}

fn detail_of(entry: &IndexEntry) -> String {
    if entry.tags.is_empty() {
        return entry.description.clone();
    }
    let tags = entry.tags.join(TAG_SEPARATOR);
    if entry.description.is_empty() {
        tags
    } else {
        format!("{}{TAG_SEPARATOR}{tags}", entry.description)
    }
}

pub(super) fn index_rows(
    index: &ExtensionIndex,
    query: &str,
    installed: &[ExtensionEntry],
    working: &BTreeSet<String>,
) -> Vec<IndexRow> {
    index
        .search(query)
        .into_iter()
        .map(|entry| IndexRow {
            id: entry.id.to_string(),
            name: entry.name.clone(),
            detail: detail_of(entry),
            action: action_for(entry, installed, working),
        })
        .collect()
}

pub(super) fn age_text(age: Duration) -> String {
    if age < MINUTE {
        return JUST_NOW.to_owned();
    }
    let (count, unit) = if age < HOUR {
        (age.as_secs() / MINUTE.as_secs(), MINUTES_UNIT)
    } else if age < DAY {
        (age.as_secs() / HOUR.as_secs(), HOURS_UNIT)
    } else {
        (age.as_secs() / DAY.as_secs(), DAYS_UNIT)
    };
    format!("{count} {unit} {AGO}")
}

pub(super) fn origin_text(origin: IndexOrigin, age: Duration) -> String {
    match origin {
        IndexOrigin::Fetched => format!("{UPDATED_PREFIX} {JUST_NOW}"),
        IndexOrigin::Cached => format!("{UPDATED_PREFIX} {}", age_text(age)),
        IndexOrigin::StaleCache => format!("{OFFLINE_PREFIX} {}", age_text(age)),
    }
}

pub(super) fn problems_text(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some(ONE_PROBLEM.to_owned()),
        many => Some(format!("{many} {PROBLEMS_SUFFIX}")),
    }
}

pub(super) fn failure_text(repository: &IndexRepository, failure: &LoadFailure) -> String {
    match failure {
        LoadFailure::Missing => format!("{MISSING_PREFIX} {repository} {MISSING_SUFFIX}."),
        LoadFailure::Unreachable(reason) => {
            format!("{UNREACHABLE_PREFIX} {repository} {UNREACHABLE_SUFFIX}: {reason}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::GitCommit;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const OLD: &str = "0000000000000000000000000000000000000000";
    const NEW: &str = "1111111111111111111111111111111111111111";

    fn index() -> Result<ExtensionIndex, Box<dyn std::error::Error>> {
        let text = format!(
            r#"{{"extensions":[
            {{"id":"docker","name":"Docker","description":"Containers","tags":["containers","compose"],
              "repository":"https://github.com/joordih/krad-docker","commit":"{NEW}","api":"1.0"}},
            {{"id":"zig","name":"Zig","repository":"https://github.com/acme/krad-zig","commit":"{NEW}","api":"1.0"}},
            {{"id":"kube","name":"Kube","repository":"https://github.com/acme/krad-kube","commit":"{NEW}","api":"9.0"}},
            {{"id":"rust","name":"Rust","repository":"https://github.com/acme/krad-rust","commit":"{NEW}","api":"1.0"}}
        ]}}"#
        );
        Ok(ExtensionIndex::parse(&text)?)
    }

    fn pinned(id: &str, commit: &str) -> Result<ExtensionEntry, Box<dyn std::error::Error>> {
        Ok(ExtensionEntry {
            id: id.parse()?,
            source: ExtensionSource::Pinned {
                url: format!("https://github.com/acme/{id}"),
                commit: commit.parse::<GitCommit>()?,
            },
            enabled: true,
            trusted: None,
        })
    }

    fn action(rows: &[IndexRow], id: &str) -> Option<RowAction> {
        rows.iter().find(|row| row.id == id).map(|row| row.action)
    }

    #[test]
    fn rows_say_what_the_user_can_do_with_each_entry() -> TestResult {
        let installed = [pinned("docker", OLD)?, pinned("zig", NEW)?];
        let working = BTreeSet::from(["rust".to_owned()]);
        let rows = index_rows(&index()?, "", &installed, &working);
        assert_eq!(action(&rows, "docker"), Some(RowAction::Update));
        assert_eq!(action(&rows, "zig"), Some(RowAction::Installed));
        assert_eq!(action(&rows, "kube"), Some(RowAction::NeedsNewerApp));
        assert_eq!(action(&rows, "rust"), Some(RowAction::Working));
        Ok(())
    }

    #[test]
    fn rows_follow_the_search_and_show_description_and_tags() -> TestResult {
        let rows = index_rows(&index()?, "compose", &[], &BTreeSet::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].detail, "Containers · containers · compose");
        assert_eq!(rows[0].action, RowAction::Install);
        Ok(())
    }

    #[test]
    fn ages_read_in_the_largest_whole_unit() {
        assert_eq!(age_text(Duration::from_secs(5)), JUST_NOW);
        assert_eq!(age_text(MINUTE * 5), "5 min ago");
        assert_eq!(age_text(HOUR * 2 + MINUTE * 10), "2 h ago");
        assert_eq!(age_text(DAY * 3), "3 d ago");
        assert_eq!(
            origin_text(IndexOrigin::StaleCache, HOUR * 2),
            "Offline, showing the index from 2 h ago"
        );
        assert_eq!(origin_text(IndexOrigin::Fetched, HOUR), "Updated just now");
    }

    #[test]
    fn problems_and_failures_read_plainly() {
        assert_eq!(problems_text(0), None);
        assert_eq!(problems_text(1).as_deref(), Some(ONE_PROBLEM));
        assert_eq!(
            problems_text(3).as_deref(),
            Some("3 entries of the index could not be read")
        );
        let repository = IndexRepository::default();
        assert_eq!(
            failure_text(&repository, &LoadFailure::Missing),
            "There is no extension index at joordih/krad-extensions-index yet."
        );
    }
}
