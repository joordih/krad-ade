use futures::executor::block_on;
use krad_domain::{EnvironmentData, EnvironmentView, Layout, PersistedLayout, WorkMode};
use krad_protocol::Scope;
use krad_runtime::{EnvironmentsSetup, Runtime, RuntimeSetup};
use krad_store::{open_layout_in, LayoutFile, LayoutOpening};
use std::error::Error;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn Error>>;

const SINGLE_FILE_NAME: &str = "layout.json";

struct Fixture {
    file: &'static str,
    version: u32,
    readable: usize,
    unreadable: usize,
}

const FIXTURES: [Fixture; 3] = [
    Fixture {
        file: "layout-v10.json",
        version: 10,
        readable: 1,
        unreadable: 0,
    },
    Fixture {
        file: "layout-v11.json",
        version: 11,
        readable: 2,
        unreadable: 0,
    },
    Fixture {
        file: "layout-v12.json",
        version: 12,
        readable: 2,
        unreadable: 1,
    },
];

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn fixture_bytes(file: &str) -> std::io::Result<Vec<u8>> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/layouts")
            .join(file),
    )
}

fn folder_with(fixture: &Fixture, bytes: &[u8]) -> std::io::Result<Folder> {
    let dir = std::env::temp_dir().join(format!(
        "krad-runtime-migration-{}-{}",
        fixture.version,
        std::process::id()
    ));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(SINGLE_FILE_NAME), bytes)?;
    Ok(Folder(dir))
}

fn rejoined(layout: Layout) -> Vec<EnvironmentData> {
    let mut view = layout.view;
    layout
        .state
        .environments
        .into_iter()
        .map(|mut environment| {
            view.dress(&mut environment);
            environment
        })
        .collect()
}

#[test]
fn single_file_layouts_of_every_version_migrate_without_losing_anything() -> TestResult {
    for fixture in &FIXTURES {
        let original = fixture_bytes(fixture.file)?;
        let single: PersistedLayout = serde_json::from_slice(&original)?;
        assert_eq!(single.version, fixture.version, "{}", fixture.file);
        assert_eq!(
            single.environments.len(),
            fixture.readable,
            "{}",
            fixture.file
        );
        assert_eq!(
            single.unreadable.len(),
            fixture.unreadable,
            "{}",
            fixture.file
        );
        let whole = single.environments.clone();
        let (active, work_mode) = (single.active.clone(), single.work_mode);
        let folder = folder_with(fixture, &original)?;
        let migrated = open_layout_in(&folder.0);
        let LayoutOpening::Migrated { from, kept } = &migrated.opening else {
            return Err(format!("{} not migrated: {:?}", fixture.file, migrated.opening).into());
        };
        assert_eq!(*from, fixture.version);
        assert_eq!(std::fs::read(kept)?, original, "{}", fixture.file);
        assert!(!folder.0.join(SINGLE_FILE_NAME).exists());
        let reopened = open_layout_in(&folder.0);
        assert!(matches!(reopened.opening, LayoutOpening::Read));
        assert_eq!(reopened.layout.view.active, Some(active));
        assert_eq!(reopened.layout.view.work_mode, work_mode);
        assert_eq!(
            reopened.layout.state.unreadable.len(),
            fixture.unreadable,
            "{}",
            fixture.file
        );
        assert_eq!(rejoined(reopened.layout), whole, "{}", fixture.file);
    }
    Ok(())
}

#[test]
fn the_runtime_starts_from_a_migrated_layout_and_keeps_the_view_to_the_ui() -> TestResult {
    let fixture = &FIXTURES[2];
    let folder = folder_with(fixture, &fixture_bytes(fixture.file)?)?;
    let opened = open_layout_in(&folder.0).layout;
    assert_eq!(opened.view.work_mode, WorkMode::Editor);
    let runtime = Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup {
            state: opened.state.clone(),
            file: Some(LayoutFile::State.path_in(&folder.0)),
        },
        ..RuntimeSetup::default()
    })?;
    let environments = block_on(runtime.snapshot(Scope::Runtime))?.environments;
    assert_eq!(environments, opened.state.environments);
    assert!(environments
        .iter()
        .all(|environment| environment.view == EnvironmentView::Sessions
            && environment.focus.is_none()
            && !environment.side_panel.open));
    let written = std::fs::read_to_string(LayoutFile::State.path_in(&folder.0))?;
    assert!(written.contains("-oProxyCommand=evil"), "{written}");
    Ok(())
}
