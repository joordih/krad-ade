use krad_domain::{
    Layout, PersistedLayout, RuntimeState, ViewPreferences, LAST_SINGLE_FILE_VERSION,
    LAYOUT_SCHEMA_VERSION,
};
use std::io;
use std::path::{Path, PathBuf};

use super::{write_atomically, LayoutFile};
use crate::config_dir::config_dir;

const SINGLE_FILE_NAME: &str = "layout.json";
const SINGLE_FILE_STEM: &str = "layout";
const SET_ASIDE_STEM: &str = "state.unread";
const JSON_EXTENSION: &str = "json";
const SET_ASIDE_ATTEMPTS: u32 = 100;

#[derive(Debug)]
pub enum LayoutOpening {
    Empty,
    Read,
    Migrated { from: u32, kept: PathBuf },
    MigratedUnsaved { from: u32, error: io::Error },
    SingleFileFromNewerApp { version: u32 },
    StateSetAside { kept: PathBuf },
    StateUnreadable { error: io::Error },
}

#[derive(Debug)]
pub struct OpenedLayout {
    pub layout: Layout,
    pub opening: LayoutOpening,
}

#[must_use]
pub fn open_layout() -> OpenedLayout {
    match config_dir() {
        Some(dir) => open_layout_in(&dir),
        None => OpenedLayout {
            layout: Layout::default(),
            opening: LayoutOpening::Empty,
        },
    }
}

#[must_use]
pub fn open_layout_in(dir: &Path) -> OpenedLayout {
    let state_path = LayoutFile::State.path_in(dir);
    match std::fs::read(&state_path) {
        Ok(bytes) => read_split(dir, &state_path, &bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => migrate_single_file(dir),
        Err(error) => OpenedLayout {
            layout: Layout::default(),
            opening: LayoutOpening::StateUnreadable { error },
        },
    }
}

fn read_split(dir: &Path, state_path: &Path, bytes: &[u8]) -> OpenedLayout {
    let view = read_view(dir);
    let state = serde_json::from_slice::<RuntimeState>(bytes)
        .ok()
        .filter(|state| state.version <= LAYOUT_SCHEMA_VERSION);
    if let Some(state) = state {
        return OpenedLayout {
            layout: Layout { state, view },
            opening: LayoutOpening::Read,
        };
    }
    let opening = match set_aside(dir, state_path) {
        Ok(kept) => LayoutOpening::StateSetAside { kept },
        Err(error) => LayoutOpening::StateUnreadable { error },
    };
    OpenedLayout {
        layout: Layout {
            state: RuntimeState::default(),
            view,
        },
        opening,
    }
}

fn read_view(dir: &Path) -> ViewPreferences {
    std::fs::read(LayoutFile::View.path_in(dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ViewPreferences>(&bytes).ok())
        .filter(|view| view.version <= LAYOUT_SCHEMA_VERSION)
        .unwrap_or_default()
}

fn set_aside(dir: &Path, state_path: &Path) -> io::Result<PathBuf> {
    let free = (0..SET_ASIDE_ATTEMPTS)
        .map(|attempt| match attempt {
            0 => dir.join(format!("{SET_ASIDE_STEM}.{JSON_EXTENSION}")),
            n => dir.join(format!("{SET_ASIDE_STEM}.{n}.{JSON_EXTENSION}")),
        })
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| io::Error::from(io::ErrorKind::AlreadyExists))?;
    std::fs::rename(state_path, &free)?;
    Ok(free)
}

fn migrate_single_file(dir: &Path) -> OpenedLayout {
    let single_path = dir.join(SINGLE_FILE_NAME);
    let single = std::fs::read(&single_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PersistedLayout>(&bytes).ok());
    let Some(single) = single else {
        return without_state(dir, LayoutOpening::Empty);
    };
    let from = single.version;
    if from > LAST_SINGLE_FILE_VERSION {
        return without_state(dir, LayoutOpening::SingleFileFromNewerApp { version: from });
    }
    let layout = Layout::from(single);
    let opening = match save_split(dir, &layout, &single_path, from) {
        Ok(kept) => LayoutOpening::Migrated { from, kept },
        Err(error) => LayoutOpening::MigratedUnsaved { from, error },
    };
    OpenedLayout { layout, opening }
}

fn without_state(dir: &Path, opening: LayoutOpening) -> OpenedLayout {
    OpenedLayout {
        layout: Layout {
            state: RuntimeState::default(),
            view: read_view(dir),
        },
        opening,
    }
}

fn save_split(dir: &Path, layout: &Layout, single_path: &Path, from: u32) -> io::Result<PathBuf> {
    let view = serde_json::to_vec_pretty(&layout.view).map_err(io::Error::other)?;
    let state = serde_json::to_vec_pretty(&layout.state).map_err(io::Error::other)?;
    write_atomically(&LayoutFile::View.path_in(dir), &view)?;
    write_atomically(&LayoutFile::State.path_in(dir), &state)?;
    let kept = dir.join(format!("{SINGLE_FILE_STEM}.v{from}.{JSON_EXTENSION}"));
    std::fs::rename(single_path, &kept)?;
    Ok(kept)
}
