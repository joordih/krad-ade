use krad_chat::{claude_skills, skill_library};
use krad_host::HostLink;
use krad_protocol::{Answer, DiffOf, ErrorCode, RepoAsk};
use krad_repo::{
    commit_diff, commit_files, commit_log, detect_tasks, diff_between, read_text,
    resolved_remote_url, working_diff, workplace_home, RepoError,
};
use std::path::{Path, PathBuf};

const UNKNOWN_ASK: &str = "this repository ask is not known to the runtime";
const NO_HOME_TEXT: &str = "The host's home folder is not known yet";

fn home_of(link: &HostLink) -> Option<PathBuf> {
    match link {
        HostLink::Local => std::env::home_dir(),
        HostLink::Remote(ssh) => ssh.home().map(Path::to_path_buf),
    }
}

fn folders_in(link: &HostLink, dir: &Path) -> Result<Vec<String>, RepoError> {
    let mut names: Vec<String> = link
        .read_dir(dir)?
        .into_iter()
        .filter(|entry| entry.metadata.is_dir())
        .map(|entry| entry.name)
        .collect();
    names.sort_by_cached_key(|name| name.to_lowercase());
    Ok(names)
}

pub(crate) fn read_answer(link: &HostLink, ask: RepoAsk) -> Result<Answer, RepoError> {
    Ok(match ask {
        RepoAsk::RemoteUrls { roots, remote } => Answer::RemoteUrls {
            urls: roots
                .iter()
                .map(|root| {
                    let home = workplace_home(link, Some(root)).ok().flatten();
                    resolved_remote_url(link, root, &remote, home.as_deref())
                        .map_err(|error| error.to_string())
                })
                .collect(),
        },
        RepoAsk::CommitLog {
            repo,
            revisions,
            limit,
        } => {
            let revisions: Vec<&str> = revisions.iter().map(String::as_str).collect();
            Answer::Commits {
                commits: commit_log(link, &repo, &revisions, limit)?,
            }
        }
        RepoAsk::CommitFiles { repo, sha } => Answer::CommitFiles {
            files: commit_files(link, &repo, &sha)?,
        },
        RepoAsk::Diff { repo, of } => Answer::Diff {
            text: match of {
                DiffOf::Working { change } => working_diff(link, &repo, &change)?,
                DiffOf::Commit { sha, change } => commit_diff(link, &repo, &sha, &change)?,
            },
        },
        RepoAsk::DiffsBetween {
            repo,
            base,
            head,
            files,
        } => Answer::Diffs {
            texts: files
                .iter()
                .map(|paths| {
                    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
                    diff_between(link, &repo, &base.0, &head.0, &paths)
                })
                .collect::<Result<_, _>>()?,
        },
        RepoAsk::Tasks { root } => {
            let detection = detect_tasks(link, &root);
            for (file, error) in &detection.unreadable {
                tracing::warn!(file = %file.display(), %error, "project task file unreadable");
            }
            Answer::Tasks {
                tasks: detection.tasks,
            }
        }
        RepoAsk::ReadText { path, max_bytes } => {
            let file = read_text(link, &path, max_bytes)?;
            Answer::Text {
                text: file.text,
                truncated: file.truncated,
            }
        }
        RepoAsk::Skills { root } => {
            let home = workplace_home(link, root.as_deref())?;
            Answer::Skills {
                skills: skill_library(link, home.as_deref(), root.as_deref()),
            }
        }
        RepoAsk::ChatSkills { root } => {
            let home = workplace_home(link, root.as_deref())?;
            Answer::ChatSkills {
                skills: claude_skills(link, home.as_deref(), root.as_deref()),
            }
        }
        RepoAsk::Folders { dir } => {
            let Some(dir) = dir.or_else(|| home_of(link)) else {
                return Ok(Answer::Failed {
                    code: ErrorCode::Unavailable,
                    message: NO_HOME_TEXT.to_owned(),
                });
            };
            let names = link
                .metadata(&dir)?
                .is_dir()
                .then(|| folders_in(link, &dir))
                .transpose()?;
            Answer::Folders { dir, names }
        }
        _ => Answer::Failed {
            code: ErrorCode::Unsupported,
            message: UNKNOWN_ASK.to_owned(),
        },
    })
}

pub(crate) fn replace_text(
    link: &HostLink,
    path: &Path,
    expected: &str,
    text: &str,
) -> Result<Answer, RepoError> {
    let limit = u64::try_from(expected.len()).unwrap_or(u64::MAX);
    let current = read_text(link, path, limit)?;
    if current.truncated || current.text != expected {
        return Ok(Answer::TextReplaced { replaced: false });
    }
    link.write(path, text.as_bytes())?;
    Ok(Answer::TextReplaced { replaced: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn only_folders_are_listed_sorted_without_case() -> TestResult {
        let dir = std::env::temp_dir().join(format!("krad-runtime-folders-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        for folder in ["beta", "Alpha", "gamma"] {
            std::fs::create_dir_all(dir.join(folder))?;
        }
        std::fs::write(dir.join("notes.txt"), "x")?;
        let listed = read_answer(
            &HostLink::Local,
            RepoAsk::Folders {
                dir: Some(dir.clone()),
            },
        )?;
        let file = read_answer(
            &HostLink::Local,
            RepoAsk::Folders {
                dir: Some(dir.join("notes.txt")),
            },
        )?;
        std::fs::remove_dir_all(&dir)?;
        let names = ["Alpha", "beta", "gamma"].map(str::to_owned).to_vec();
        assert_eq!(
            listed,
            Answer::Folders {
                dir: dir.clone(),
                names: Some(names)
            }
        );
        assert_eq!(
            file,
            Answer::Folders {
                dir: dir.join("notes.txt"),
                names: None
            }
        );
        Ok(())
    }
}
