use krad_client::ClientError;
use krad_domain::{ReplacedFile, UndoOutcome};
use krad_protocol::Answer;
use std::path::PathBuf;

use super::{ReplaceReport, UndoReport};

pub(super) fn replaced_of(answer: Answer) -> Option<Vec<Result<ReplacedFile, String>>> {
    match answer {
        Answer::Replaced { files } => Some(files),
        _ => None,
    }
}

pub(super) fn undone_of(answer: Answer) -> Option<Vec<Result<UndoOutcome, String>>> {
    match answer {
        Answer::Undone { outcomes } => Some(outcomes),
        _ => None,
    }
}

pub(super) fn replace_report(
    paths: &[PathBuf],
    replaced: Result<Vec<Result<ReplacedFile, String>>, ClientError>,
) -> ReplaceReport {
    let mut report = ReplaceReport::default();
    match replaced {
        Ok(files) => {
            for (path, file) in paths.iter().zip(files) {
                match file {
                    Ok(replaced) => report.add(replaced),
                    Err(error) => report.failures.push(format!("{}: {error}", path.display())),
                }
            }
        }
        Err(error) => report.failures.push(error.to_string()),
    }
    report
}

pub(super) fn undo_report(
    paths: &[PathBuf],
    undone: Result<Vec<Result<UndoOutcome, String>>, ClientError>,
) -> UndoReport {
    let mut report = UndoReport::default();
    match undone {
        Ok(outcomes) => {
            for (path, outcome) in paths.iter().zip(outcomes) {
                match outcome {
                    Ok(UndoOutcome::Restored) => report.restored += 1,
                    Ok(UndoOutcome::ChangedSince) => report.changed_since += 1,
                    Err(error) => report.failures.push(format!("{}: {error}", path.display())),
                }
            }
        }
        Err(error) => report.failures.push(error.to_string()),
    }
    report
}
