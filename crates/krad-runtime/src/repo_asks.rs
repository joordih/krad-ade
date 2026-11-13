mod reads;

use krad_host::HostLink;
use krad_protocol::{Answer, ErrorCode, RepoAsk, WorktreeChecked, WorktreeRemoval};
use krad_repo::{
    add_note, add_worktree, checkpoint_changes, discard_worktree, docs_source_candidates,
    read_docs_sources, read_session_templates, remove_worktree, restore_checkpoint,
    reveal_in_file_manager, worktree_state, write_docs_sources, RepoError,
};

pub(crate) use reads::{read_answer, replace_text};

pub(crate) fn failed(error: &RepoError) -> Answer {
    let unavailable = match error {
        RepoError::Io(_) => true,
        RepoError::Host(error) => error.breaks_the_channel(),
        _ => false,
    };
    Answer::Failed {
        code: if unavailable {
            ErrorCode::Unavailable
        } else {
            ErrorCode::Failed
        },
        message: error.to_string(),
    }
}

fn answered(answer: Result<Answer, RepoError>) -> Answer {
    answer.unwrap_or_else(|error| failed(&error))
}

#[tracing::instrument(skip_all)]
pub(crate) fn answer(link: &HostLink, ask: RepoAsk) -> Answer {
    answered(match ask {
        RepoAsk::DocsSources { root } => read_docs_sources(link, &root).and_then(|picked| {
            Ok(Answer::DocsSources {
                picked,
                candidates: docs_source_candidates(link, &root)?,
            })
        }),
        RepoAsk::SaveDocsSources { root, sources } => {
            write_docs_sources(link, &root, &sources).map(|()| Answer::Done)
        }
        RepoAsk::SessionTemplates { root } => {
            read_session_templates(link, &root).map(|templates| Answer::Templates { templates })
        }
        RepoAsk::AddWorktree { repo, name } => {
            add_worktree(link, &repo, &name).map(|worktree| Answer::WorktreeAdded { worktree })
        }
        RepoAsk::WorktreeStates { base, worktrees } => Ok(Answer::WorktreeStates {
            states: worktrees
                .into_iter()
                .map(|probe| WorktreeChecked {
                    state: worktree_state(
                        link,
                        &probe.main,
                        &probe.path,
                        probe.branch.as_deref(),
                        base,
                    )
                    .map_err(|error| error.to_string()),
                    path: probe.path,
                })
                .collect(),
        }),
        RepoAsk::RemoveWorktree {
            repo,
            path,
            removal,
        } => match removal {
            WorktreeRemoval::Clean => remove_worktree(link, &repo, &path),
            WorktreeRemoval::Discard => discard_worktree(link, &repo, &path),
        }
        .map(|()| Answer::Done),
        RepoAsk::AddNote {
            root,
            folder,
            stem,
            text,
        } => add_note(link, &root, &folder, &stem, &text).map(|name| Answer::NoteAdded { name }),
        RepoAsk::CheckpointChanges { repo, checkpoint } => {
            checkpoint_changes(link, &repo, &checkpoint)
                .map(|changes| Answer::CheckpointChanges { changes })
        }
        RepoAsk::RestoreCheckpoint { repo, checkpoint } => {
            restore_checkpoint(link, &repo, &checkpoint)
                .map(|restored| Answer::CheckpointRestored { restored })
        }
        RepoAsk::ReplaceText {
            path,
            expected,
            text,
        } => replace_text(link, &path, &expected, &text),
        RepoAsk::Reveal { path } => reveal_in_file_manager(link, &path).map(|()| Answer::Done),
        read => read_answer(link, read),
    })
}
