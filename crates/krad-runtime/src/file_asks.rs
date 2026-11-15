mod containment;
mod notes;
mod reads;
mod uploads;

use krad_domain::{EntryKind, FileStat};
use krad_host::{HostError, HostLink, Metadata};
use krad_protocol::{
    Answer, ErrorCode, FileAsk, NewFile, TextRead, MAX_ATTACHMENT_BYTES, MAX_FOUND_LINES,
    MAX_READ_BYTES, MAX_TEXTS_READ, MAX_WRITE_BYTES,
};
use krad_repo::{
    create_dir, create_file, delete_entry, index_files, move_entry, read_text, relocate_entry,
    rename_entry, replace_in_file, search_project, search_text, undo_replace, RepoError,
};
use std::path::{Path, PathBuf};

pub(crate) use uploads::{upload, Uploads};

const TOO_LARGE_PUT: &str = "the file is too large to upload";
const TOO_LARGE_TO_WRITE: &str = "the text is too large to write";
const ALREADY_EXISTS_TEXT: &str = "already exists";
const UNKNOWN_ASK: &str = "this file ask is not known to the runtime";

pub(crate) fn failed(error: &RepoError) -> Answer {
    let code = match error {
        error if error.is_not_found() => ErrorCode::NotFound,
        RepoError::Host(error) if error.breaks_the_channel() => ErrorCode::Unavailable,
        _ => ErrorCode::Failed,
    };
    Answer::Failed {
        code,
        message: error.to_string(),
    }
}

fn refused(code: ErrorCode, message: &str) -> Answer {
    Answer::Failed {
        code,
        message: message.to_owned(),
    }
}

pub(crate) fn stat_of(metadata: &Metadata) -> FileStat {
    FileStat {
        kind: if metadata.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        },
        len: metadata.len,
        modified: metadata.modified,
    }
}

fn stat(link: &HostLink, path: &std::path::Path) -> Result<Option<FileStat>, HostError> {
    match link.metadata(path) {
        Ok(metadata) => Ok(Some(stat_of(&metadata))),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error),
    }
}

fn create_files(
    link: &HostLink,
    folders: &[std::path::PathBuf],
    files: &[NewFile],
) -> Result<Answer, RepoError> {
    for folder in folders {
        link.create_dir_all(folder)?;
    }
    for file in files {
        if link.exists(&file.path) {
            return Ok(refused(
                ErrorCode::Failed,
                &format!("{} {ALREADY_EXISTS_TEXT}", file.path.display()),
            ));
        }
        if let Some(parent) = file.path.parent() {
            link.create_dir_all(parent)?;
        }
        link.write(&file.path, file.text.as_bytes())?;
    }
    Ok(Answer::Done)
}

#[tracing::instrument(skip_all)]
pub(crate) fn answer(link: &HostLink, ask: FileAsk) -> Answer {
    match ask {
        FileAsk::Write { text, .. } if text.len() > MAX_WRITE_BYTES => {
            refused(ErrorCode::Invalid, TOO_LARGE_TO_WRITE)
        }
        ask => worked(link, ask).unwrap_or_else(|error| failed(&error)),
    }
}

fn put(link: &HostLink, dir: &Path, name: &str, bytes: &[u8]) -> Result<Answer, RepoError> {
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Ok(Answer::Failed {
            code: ErrorCode::Invalid,
            message: TOO_LARGE_PUT.to_owned(),
        });
    }
    let path = create_file(link, dir, name)?;
    link.write(&path, bytes)?;
    Ok(Answer::Placed { path })
}

fn read_texts(link: &HostLink, paths: Vec<PathBuf>, limit: u64) -> Answer {
    Answer::Texts {
        texts: paths
            .into_iter()
            .take(MAX_TEXTS_READ)
            .filter_map(|path| {
                let file = read_text(link, &path, limit.min(MAX_READ_BYTES)).ok()?;
                Some(TextRead {
                    path,
                    text: file.text,
                    truncated: file.truncated,
                })
            })
            .collect(),
    }
}

fn worked(link: &HostLink, ask: FileAsk) -> Result<Answer, RepoError> {
    Ok(match ask {
        FileAsk::Read {
            path,
            from,
            limit,
            over,
        } => reads::read(link, &path, (from, limit), over)?,
        FileAsk::Stat { path } => Answer::Stat {
            stat: stat(link, &path)?,
        },
        FileAsk::Write { path, text } => {
            link.write(&path, text.as_bytes())?;
            Answer::Written {
                stat: stat(link, &path)?,
            }
        }
        FileAsk::List { dir } => Answer::Entries {
            entries: krad_repo::list_dir(link, &dir)?,
        },
        FileAsk::CreateFile { dir, name } => Answer::Placed {
            path: create_file(link, &dir, &name)?,
        },
        FileAsk::Put { dir, name, bytes } => put(link, &dir, &name, &bytes)?,
        FileAsk::CreateDir { dir, name } => Answer::Placed {
            path: create_dir(link, &dir, &name)?,
        },
        FileAsk::Rename { path, name } => Answer::Placed {
            path: rename_entry(link, &path, &name)?,
        },
        FileAsk::Move { path, into } => Answer::Placed {
            path: move_entry(link, &path, &into)?,
        },
        FileAsk::Relocate { path, target } => Answer::Placed {
            path: relocate_entry(link, &path, &target)?,
        },
        FileAsk::Delete { path } => {
            delete_entry(link, &path)?;
            Answer::Done
        }
        FileAsk::Taken { dir, names } => Answer::Taken {
            names: names
                .into_iter()
                .filter(|name| link.symlink_metadata(&dir.join(name)).is_ok())
                .collect(),
        },
        FileAsk::Contained { root, path } => containment::contained(link, &root, &path),
        FileAsk::Present { paths } => Answer::Present {
            present: paths.iter().map(|path| link.exists(path)).collect(),
        },
        FileAsk::ReadTexts { paths, limit } => read_texts(link, paths, limit),
        FileAsk::Index { root } => Answer::Indexed {
            files: index_files(link, &root)?
                .files()
                .map(std::path::Path::to_path_buf)
                .collect(),
        },
        FileAsk::CreateFiles { folders, files } => create_files(link, &folders, &files)?,
        FileAsk::Search { root, query, limit } => Answer::ProjectMatches {
            matches: search_project(link, &root, &query, limit.min(MAX_FOUND_LINES))?,
        },
        FileAsk::Replace { files, replacement } => Answer::Replaced {
            files: files
                .iter()
                .map(|file| {
                    replace_in_file(link, file, &replacement).map_err(|error| error.to_string())
                })
                .collect(),
        },
        FileAsk::UndoReplace { undos } => Answer::Undone {
            outcomes: undos
                .iter()
                .map(|undo| undo_replace(link, undo).map_err(|error| error.to_string()))
                .collect(),
        },
        FileAsk::SearchText { dir, query, limit } => Answer::TextMatches {
            matches: search_text(link, &dir, &query, limit.min(MAX_FOUND_LINES))?,
        },
        notes @ (FileAsk::Vault { .. }
        | FileAsk::CreateNote { .. }
        | FileAsk::CreateVaultFolder { .. }
        | FileAsk::WriteNote { .. }
        | FileAsk::SaveAttachment { .. }) => notes::answer(link, notes)?,
        FileAsk::Upload { .. }
        | FileAsk::CancelUpload { .. }
        | FileAsk::Watch { .. }
        | FileAsk::Unwatch { .. }
        | FileAsk::Unknown => refused(ErrorCode::Unsupported, UNKNOWN_ASK),
    })
}
