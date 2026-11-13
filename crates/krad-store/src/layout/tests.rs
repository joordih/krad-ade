use super::*;
use krad_domain::{
    ArchivedSession, ArchivedSessionId, EnvironmentData, EnvironmentName, GroupId, Layout,
    PersistedLayout, RuntimeState, SessionLook, ViewPreferences, WorkMode,
};
use std::time::{Duration, SystemTime};

const WRITERS: usize = 8;
const WRITES_EACH: usize = 20;
const NEWER_SINGLE_FILE_VERSION: u32 = krad_domain::LAST_SINGLE_FILE_VERSION + 1;

fn fresh_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("krad-ade-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    dir
}

fn single_file(version: u32) -> serde_json::Value {
    serde_json::json!({
        "version": version,
        "active": "main",
        "work_mode": "Editor",
        "environments": [{
            "name": "main",
            "nodes": [{ "id": 2, "title": "claude", "state": "Idle", "harness": "ClaudeCode" }],
            "meta": { "2": { "cwd": "/work/api", "scrollback": ["ok"] } },
            "next_id": 3,
            "view": "Notes",
            "focus": 2,
            "groups": [{ "id": 1, "name": "api", "root": "/work/api" }],
            "side_panel": { "open": true, "width": 380.0 }
        }]
    })
}

#[test]
fn writers_racing_on_one_file_never_share_a_temp_file() -> std::io::Result<()> {
    let dir = fresh_dir("atomic");
    let path = dir.join("digests.json");
    let payloads: Vec<String> = (0..WRITERS)
        .map(|writer| format!("{{\"writer\":{writer}}}"))
        .collect();
    let outcomes: Vec<std::io::Result<()>> = std::thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .iter()
            .map(|payload| {
                let path = &path;
                scope.spawn(move || {
                    (0..WRITES_EACH).try_for_each(|_| write_atomically(path, payload.as_bytes()))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(std::io::Error::other("a writer panicked")))
            })
            .collect()
    });
    for outcome in outcomes {
        outcome?;
    }
    let kept = std::fs::read_to_string(&path)?;
    assert!(payloads.contains(&kept), "{kept}");
    let left: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(left.len(), 1, "{left:?}");
    std::fs::remove_dir_all(&dir)
}

#[test]
fn writer_lands_the_newest_snapshot() -> std::io::Result<()> {
    let dir = fresh_dir("writer");
    let path = LayoutFile::View.path_in(&dir);
    let view = |name: &str| ViewPreferences {
        active: Some(EnvironmentName::new(name.to_owned())),
        ..ViewPreferences::default()
    };
    let writer = LayoutWriter::spawn_at(path.clone());
    assert!(writer.is_some(), "a writer thread starts");
    if let Some(writer) = writer {
        writer.persist(&view("older"));
        writer.persist(&view("newer"));
    }
    let opened = open_layout_in(&dir);
    assert_eq!(opened.layout.view.active, view("newer").active);
    std::fs::remove_dir_all(&dir)
}

#[test]
fn a_folder_without_layout_files_opens_empty() {
    let opened = open_layout_in(&fresh_dir("empty"));
    assert!(matches!(opened.opening, LayoutOpening::Empty));
    assert_eq!(opened.layout, Layout::default());
}

#[test]
fn a_single_file_layout_is_split_once_and_kept_as_a_copy() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = fresh_dir("migrate");
    std::fs::create_dir_all(&dir)?;
    let original = serde_json::to_vec_pretty(&single_file(12))?;
    std::fs::write(dir.join("layout.json"), &original)?;
    let first = open_layout_in(&dir);
    let LayoutOpening::Migrated { from: 12, kept } = &first.opening else {
        return Err(format!("not migrated: {:?}", first.opening).into());
    };
    assert_eq!(std::fs::read(kept)?, original);
    assert!(!dir.join("layout.json").exists());
    assert!(LayoutFile::State.path_in(&dir).exists());
    assert!(LayoutFile::View.path_in(&dir).exists());
    let expected = Layout::from(serde_json::from_slice::<PersistedLayout>(&original)?);
    assert_eq!(first.layout, expected);
    let second = open_layout_in(&dir);
    assert!(matches!(second.opening, LayoutOpening::Read));
    let mut view = second.layout.view.clone();
    let rejoined: Vec<EnvironmentData> = second
        .layout
        .state
        .environments
        .into_iter()
        .map(|mut environment| {
            view.dress(&mut environment);
            environment
        })
        .collect();
    let whole = serde_json::from_slice::<PersistedLayout>(&original)?.environments;
    assert_eq!(rejoined, whole);
    assert_eq!(second.layout.view.work_mode, WorkMode::Editor);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn a_single_file_layout_from_a_newer_app_is_left_untouched(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = fresh_dir("newer-single");
    std::fs::create_dir_all(&dir)?;
    let original = serde_json::to_vec(&single_file(NEWER_SINGLE_FILE_VERSION))?;
    std::fs::write(dir.join("layout.json"), &original)?;
    let opened = open_layout_in(&dir);
    assert!(matches!(
        opened.opening,
        LayoutOpening::SingleFileFromNewerApp { .. }
    ));
    assert_eq!(std::fs::read(dir.join("layout.json"))?, original);
    assert!(!LayoutFile::State.path_in(&dir).exists());
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn a_state_from_a_newer_app_is_set_aside_instead_of_overwritten(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = fresh_dir("newer-state");
    std::fs::create_dir_all(&dir)?;
    let newer = RuntimeState {
        version: krad_domain::LAYOUT_SCHEMA_VERSION + 1,
        ..RuntimeState::default()
    };
    let original = serde_json::to_vec(&newer)?;
    std::fs::write(LayoutFile::State.path_in(&dir), &original)?;
    let opened = open_layout_in(&dir);
    let LayoutOpening::StateSetAside { kept } = &opened.opening else {
        return Err(format!("not set aside: {:?}", opened.opening).into());
    };
    assert_eq!(std::fs::read(kept)?, original);
    assert_eq!(opened.layout.state.environments, Vec::new());
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn an_environment_keeps_its_archive_across_a_restart() -> std::io::Result<()> {
    let dir = fresh_dir("archive");
    let closed = SystemTime::UNIX_EPOCH + Duration::from_hours(24);
    let mut environment = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
    environment.archive.sessions.push(ArchivedSession {
        id: ArchivedSessionId(1),
        group: GroupId(1),
        name: "Refunds".to_owned(),
        look: SessionLook::default(),
        closed,
        chats: Vec::new(),
        shells: Vec::new(),
        files: Vec::new(),
        lead: None,
        workers: Vec::new(),
        cloud: None,
    });
    let kept = environment.archive.clone();
    let state = RuntimeState {
        environments: vec![environment],
        ..RuntimeState::default()
    };
    let writer = LayoutWriter::spawn_at(LayoutFile::State.path_in(&dir));
    if let Some(writer) = &writer {
        writer.persist(&state);
    }
    drop(writer);
    let back = open_layout_in(&dir).layout.state;
    assert_eq!(
        back.environments
            .into_iter()
            .next()
            .map(|environment| environment.archive),
        Some(kept)
    );
    std::fs::remove_dir_all(&dir)
}

#[test]
fn a_waiter_hears_once_the_newest_snapshot_is_on_disk() -> std::io::Result<()> {
    let dir = fresh_dir("then");
    let path = LayoutFile::View.path_in(&dir);
    let (heard, hear) = std::sync::mpsc::channel();
    let writer = LayoutWriter::spawn_at(path);
    assert!(writer.is_some(), "a writer thread starts");
    if let Some(writer) = &writer {
        writer.persist(&ViewPreferences {
            work_mode: WorkMode::Editor,
            ..ViewPreferences::default()
        });
        writer.then(move || heard.send(()).unwrap_or(()));
    }
    assert!(hear.recv_timeout(Duration::from_secs(5)).is_ok());
    assert_eq!(open_layout_in(&dir).layout.view.work_mode, WorkMode::Editor);
    drop(writer);
    std::fs::remove_dir_all(&dir)
}

#[test]
fn a_reporting_writer_tells_a_failed_write() -> std::io::Result<()> {
    let dir = fresh_dir("reporting");
    std::fs::create_dir_all(&dir)?;
    let blocker = dir.join("not-a-folder");
    std::fs::write(&blocker, b"")?;
    let (failed, hear) = std::sync::mpsc::channel();
    let writer = LayoutWriter::spawn_reporting(blocker.join("settings.json"), move |error| {
        failed.send(error.kind()).unwrap_or(());
    });
    assert!(writer.is_some(), "a writer thread starts");
    if let Some(writer) = &writer {
        writer.persist(&ViewPreferences::default());
    }
    assert!(hear.recv_timeout(Duration::from_secs(5)).is_ok());
    drop(writer);
    std::fs::remove_dir_all(&dir)
}
