use gpui_kit::Context;
use krad_client::ClientError;
use krad_domain::agents_api::{
    target_group, ApiError, ErrorCode, FilePosition, Request, Response, TerminalHandle,
};
use krad_domain::{EnvironmentName, GroupId, TerminalId};
use krad_lsp::lsp_types::Position;
use krad_protocol::{Answer, Command, ErrorCode as Failed, FileAsk, FileCommand};
use krad_runtime::BusReply;
use std::path::{Component, Path, PathBuf};

use super::super::EnvironmentShell;

const NOT_A_WORKSPACE_REQUEST: &str = "Not a workspace request";
const NO_ENVIRONMENT: &str = "The caller's environment is not open";
const NO_SUCH_GROUP: &str = "No group with that name in your environment";
const NOT_ON_SCREEN: &str = "That environment is not the one on screen; open it in the app first";
const OUTSIDE_GROUP: &str = "The file is not inside the caller's group folder";
const UNRESOLVED_GROUP: &str = "The caller's group folder could not be read";
const UNEXPECTED_ANSWER: &str = "The runtime answered the file check with something else";
const LINE_BASE: u32 = 1;

struct CheckedFile {
    root: PathBuf,
    path: PathBuf,
}

fn refusal(code: ErrorCode, message: &str) -> ApiError {
    ApiError::new(code, message)
}

fn no_environment() -> ApiError {
    refusal(ErrorCode::NotFound, NO_ENVIRONMENT)
}

pub(super) fn file_in_group(root: &Path, requested: &Path) -> Result<PathBuf, ApiError> {
    let climbs = requested
        .components()
        .any(|component| component == Component::ParentDir);
    let path = root.join(requested);
    if climbs || !path.starts_with(root) {
        return Err(refusal(ErrorCode::Forbidden, OUTSIDE_GROUP));
    }
    Ok(path)
}

fn containment(answer: Result<Answer, ClientError>) -> Result<(), ApiError> {
    match answer {
        Ok(Answer::Done) => Ok(()),
        Ok(_) => Err(refusal(ErrorCode::Failed, UNEXPECTED_ANSWER)),
        Err(ClientError::Failed {
            code: Failed::Forbidden,
            ..
        }) => Err(refusal(ErrorCode::Forbidden, OUTSIDE_GROUP)),
        Err(ClientError::Failed {
            code: Failed::NotFound,
            ..
        }) => Err(refusal(ErrorCode::NotFound, UNRESOLVED_GROUP)),
        Err(error) => Err(refusal(ErrorCode::Unavailable, &error.to_string())),
    }
}

fn editor_position(position: FilePosition) -> Position {
    Position {
        line: position.line.saturating_sub(LINE_BASE),
        character: position
            .column
            .map_or(0, |column| column.saturating_sub(LINE_BASE)),
    }
}

impl EnvironmentShell {
    pub(super) fn caller_group(&self, from: &TerminalHandle) -> Option<GroupId> {
        self.envs
            .get(&EnvironmentName::new(from.environment.clone()))
            .and_then(|live| live.data.group_of(TerminalId(from.terminal)))
            .map(|group| group.id)
    }

    pub(super) fn answer_workspace(
        &mut self,
        environment: &str,
        caller: Option<GroupId>,
        request: Request,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        let env = EnvironmentName::new(environment.to_owned());
        let answered = match request {
            Request::Hello
            | Request::Sessions
            | Request::CreateSession { .. }
            | Request::CloseSession { .. } => Ok(super::answered_by_runtime()),
            Request::OpenFile { path, position } => {
                match self.checked_file_for(&env, caller, &path) {
                    Ok(checked) => {
                        Self::open_resolved_file(env, checked, position, reply, cx);
                        return;
                    }
                    Err(error) => Err(error),
                }
            }
            _ => Err(refusal(ErrorCode::BadRequest, NOT_A_WORKSPACE_REQUEST)),
        };
        reply
            .send(answered.unwrap_or_else(Response::Error))
            .unwrap_or_default();
    }

    fn checked_file_for(
        &self,
        env: &EnvironmentName,
        caller: Option<GroupId>,
        requested: &Path,
    ) -> Result<CheckedFile, ApiError> {
        let data = &self.envs.get(env).ok_or_else(no_environment)?.data;
        let group = target_group(data, caller, None)?;
        let root = data
            .group(group)
            .map(|group| group.root.clone())
            .ok_or_else(|| refusal(ErrorCode::NotFound, NO_SUCH_GROUP))?;
        let path = file_in_group(&root, requested)?;
        if *env != self.active {
            return Err(refusal(ErrorCode::Unsupported, NOT_ON_SCREEN));
        }
        Ok(CheckedFile { root, path })
    }

    fn open_resolved_file(
        env: EnvironmentName,
        checked: CheckedFile,
        position: Option<FilePosition>,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let Ok(asked) = this.update(cx, |shell, cx| {
                let command = Command::File(FileCommand {
                    environment: env.clone(),
                    ask: FileAsk::Contained {
                        root: checked.root,
                        path: checked.path.clone(),
                    },
                });
                shell.runtime.ask(command, cx)
            }) else {
                return;
            };
            let contained = containment(asked.await);
            this.update(cx, |shell, cx| {
                let answer = contained.and_then(|()| {
                    if env != shell.active {
                        return Err(refusal(ErrorCode::Unsupported, NOT_ON_SCREEN));
                    }
                    shell.open_file_at(&checked.path, position.map(editor_position), cx);
                    Ok(Response::Opened { path: checked.path })
                });
                reply
                    .send(answer.unwrap_or_else(Response::Error))
                    .unwrap_or_default();
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_files_inside_the_group_open() {
        let root = Path::new("/work/api");
        assert_eq!(
            file_in_group(root, Path::new("src/lib.rs")),
            Ok(PathBuf::from("/work/api/src/lib.rs"))
        );
        assert_eq!(
            file_in_group(root, Path::new("/work/api/README.md")),
            Ok(PathBuf::from("/work/api/README.md"))
        );
        for outside in ["../web/x.rs", "/etc/passwd", "src/../../x"] {
            assert_eq!(
                file_in_group(root, Path::new(outside)).map_err(|error| error.code),
                Err(ErrorCode::Forbidden),
                "{outside}"
            );
        }
    }

    #[test]
    fn editor_positions_count_from_zero() {
        let at = FilePosition {
            line: 12,
            column: Some(4),
        };
        assert_eq!(
            editor_position(at),
            Position {
                line: 11,
                character: 3
            }
        );
        let line_only = FilePosition {
            line: 1,
            column: None,
        };
        assert_eq!(
            editor_position(line_only),
            Position {
                line: 0,
                character: 0
            }
        );
    }
}
