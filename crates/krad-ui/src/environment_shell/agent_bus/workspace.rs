mod git;

use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::{AppContext as _, Context};
use krad_agents::{
    ApiError, ApiInfo, ErrorCode, FilePosition, Request, Response, SessionInfo, TerminalHandle,
};
use krad_domain::{EnvironmentData, EnvironmentName, GroupId, SessionId, SessionSpec, TerminalId};
use krad_host::HostLink;
use krad_lsp::lsp_types::Position;
use std::path::{Component, Path, PathBuf};

use super::super::sessions_view::session_terminals;
use super::super::EnvironmentShell;

const NOT_A_WORKSPACE_REQUEST: &str = "Not a workspace request";
const NO_ENVIRONMENT: &str = "The caller's environment is not open";
const NO_SUCH_GROUP: &str = "No group with that name in your environment";
const NO_CALLER_GROUP: &str = "The caller belongs to no group; name one with --group";
const NO_SUCH_SESSION: &str = "No session with that id in your environment";
const NO_SESSION_GROUP: &str = "That session belongs to no group";
const LAST_SESSION: &str = "That session is the last one of its group and still has tabs";
const NOT_ON_SCREEN: &str = "That environment is not the one on screen; open it in the app first";
const OUTSIDE_GROUP: &str = "The file is not inside the caller's group folder";
const UNRESOLVED_GROUP: &str = "The caller's group folder could not be read";
const LINE_BASE: u32 = 1;

struct CheckedFile {
    link: HostLink,
    root: PathBuf,
    path: PathBuf,
}

fn refusal(code: ErrorCode, message: &str) -> ApiError {
    ApiError::new(code, message)
}

fn no_environment() -> ApiError {
    refusal(ErrorCode::NotFound, NO_ENVIRONMENT)
}

pub(super) fn session_infos(data: &EnvironmentData) -> Vec<SessionInfo> {
    data.sessions
        .iter()
        .map(|session| SessionInfo {
            session: session.id.as_u64(),
            name: session.name.clone(),
            group: session
                .group
                .and_then(|id| data.group(id))
                .map(|group| group.name.clone()),
            is_current: data.active_session == Some(session.id),
            terminals: session_terminals(session).map(TerminalId::as_u64).collect(),
        })
        .collect()
}

pub(in crate::environment_shell) fn target_group(
    data: &EnvironmentData,
    caller: Option<GroupId>,
    named: Option<&str>,
) -> Result<GroupId, ApiError> {
    let Some(name) = named else {
        return caller.ok_or_else(|| refusal(ErrorCode::Unsupported, NO_CALLER_GROUP));
    };
    data.groups
        .iter()
        .find(|group| group.name == name)
        .map(|group| group.id)
        .ok_or_else(|| refusal(ErrorCode::NotFound, NO_SUCH_GROUP))
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

pub(super) fn resolved_in_group(
    link: &HostLink,
    root: &Path,
    path: &Path,
) -> Result<PathBuf, ApiError> {
    let real_root = link
        .canonicalize(root)
        .map_err(|_| refusal(ErrorCode::NotFound, UNRESOLVED_GROUP))?;
    let mut existing = path;
    let mut rest = Vec::new();
    let real_existing = loop {
        if let Ok(real) = link.canonicalize(existing) {
            break real;
        }
        let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
            return Err(refusal(ErrorCode::Forbidden, OUTSIDE_GROUP));
        };
        rest.push(name);
        existing = parent;
    };
    let real = rest
        .iter()
        .rev()
        .fold(real_existing, |real, name| real.join(name));
    if !real.starts_with(&real_root) {
        return Err(refusal(ErrorCode::Forbidden, OUTSIDE_GROUP));
    }
    Ok(path.to_path_buf())
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
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let env = EnvironmentName::new(environment.to_owned());
        let answered = match request {
            Request::Hello => Ok(Response::Hello(ApiInfo::current())),
            Request::Sessions => self
                .envs
                .get(&env)
                .map(|live| Response::Sessions(session_infos(&live.data)))
                .ok_or_else(no_environment),
            Request::CreateSession { name, group } => {
                self.create_session_for(&env, caller, group.as_deref(), name, cx)
            }
            Request::CloseSession { session } => {
                self.close_session_for(&env, SessionId(session), cx)
            }
            Request::OpenFile { path, position } => {
                match self.checked_file_for(&env, caller, &path) {
                    Ok(checked) => {
                        Self::open_resolved_file(env, checked, position, reply, cx);
                        return;
                    }
                    Err(error) => Err(error),
                }
            }
            Request::CreateWorktree { name } => {
                self.create_worktree_for(env, caller, &name, reply, cx);
                return;
            }
            Request::Changes { session } => {
                self.send_changes(&env, caller, session.map(SessionId), reply, cx);
                return;
            }
            _ => Err(refusal(ErrorCode::BadRequest, NOT_A_WORKSPACE_REQUEST)),
        };
        reply
            .send(answered.unwrap_or_else(Response::Error))
            .unwrap_or_default();
    }

    fn create_session_for(
        &mut self,
        env: &EnvironmentName,
        caller: Option<GroupId>,
        group: Option<&str>,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<Response, ApiError> {
        let live = self.envs.get_mut(env).ok_or_else(no_environment)?;
        let group = target_group(&live.data, caller, group)?;
        let shown = live.data.active_session;
        let spec = SessionSpec {
            name: name.unwrap_or_default(),
        };
        let session = live.data.new_session(Some(group), spec);
        live.data.active_session = shown;
        cx.notify();
        self.store();
        Ok(Response::SessionCreated {
            session: session.as_u64(),
        })
    }

    fn close_session_for(
        &mut self,
        env: &EnvironmentName,
        session: SessionId,
        cx: &mut Context<Self>,
    ) -> Result<Response, ApiError> {
        let data = &self.envs.get(env).ok_or_else(no_environment)?.data;
        if data.session(session).is_none() {
            return Err(refusal(ErrorCode::NotFound, NO_SUCH_SESSION));
        }
        if !data.can_close_session(session) {
            return Err(refusal(ErrorCode::Failed, LAST_SESSION));
        }
        if *env != self.active {
            return Err(refusal(ErrorCode::Unsupported, NOT_ON_SCREEN));
        }
        self.close_session(session, cx);
        Ok(Response::SessionClosed {
            session: session.as_u64(),
        })
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
        Ok(CheckedFile {
            link: self.env_link(env),
            root,
            path,
        })
    }

    fn open_resolved_file(
        env: EnvironmentName,
        checked: CheckedFile,
        position: Option<FilePosition>,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let resolved = cx.background_spawn(async move {
            resolved_in_group(&checked.link, &checked.root, &checked.path)
        });
        cx.spawn(async move |this, cx| {
            let resolved = resolved.await;
            this.update(cx, |shell, cx| {
                let answer = resolved.and_then(|path| {
                    if env != shell.active {
                        return Err(refusal(ErrorCode::Unsupported, NOT_ON_SCREEN));
                    }
                    shell.open_file_at(&path, position.map(editor_position), cx);
                    Ok(Response::Opened { path })
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

    fn environment() -> (EnvironmentData, GroupId) {
        let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
        let group = data.add_group(PathBuf::from("/work/api"), None);
        data.enter_sessions();
        (data, group)
    }

    #[test]
    fn a_session_lands_in_the_named_group_or_the_callers() {
        let (mut data, api) = environment();
        let web = data.add_group(PathBuf::from("/work/web"), None);
        let web_name = data.group(web).map(|group| group.name.clone());
        assert_eq!(target_group(&data, Some(api), None), Ok(api));
        assert_eq!(target_group(&data, Some(api), web_name.as_deref()), Ok(web));
        assert_eq!(
            target_group(&data, Some(api), Some("nope")).map_err(|error| error.code),
            Err(ErrorCode::NotFound)
        );
        assert_eq!(
            target_group(&data, None, None).map_err(|error| error.code),
            Err(ErrorCode::Unsupported)
        );
        let shown = data.active_session;
        let created = data.new_session(Some(api), SessionSpec::default());
        data.active_session = shown;
        let sessions = session_infos(&data);
        let listed = sessions
            .iter()
            .find(|session| session.session == created.as_u64());
        assert!(listed.is_some_and(|session| !session.is_current));
        assert!(listed.is_some_and(|session| session.group.is_some()));
        assert_eq!(
            sessions.iter().filter(|session| session.is_current).count(),
            usize::from(shown.is_some())
        );
    }

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

    #[cfg(unix)]
    #[test]
    fn a_link_that_leads_out_of_the_group_is_refused() -> std::io::Result<()> {
        let base = std::env::temp_dir().join(format!("krad-ade-open-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        let (root, outside) = (base.join("api"), base.join("secrets"));
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(&outside)?;
        std::fs::write(root.join("src/lib.rs"), "")?;
        std::fs::write(outside.join("key"), "")?;
        std::os::unix::fs::symlink(&outside, root.join("escape"))?;
        std::os::unix::fs::symlink(root.join("src/lib.rs"), root.join("lib.rs"))?;
        let link = HostLink::Local;
        let code = |requested: &str| {
            let path = file_in_group(&root, Path::new(requested))?;
            resolved_in_group(&link, &root, &path)
        };
        assert_eq!(code("src/lib.rs"), Ok(root.join("src/lib.rs")));
        assert_eq!(code("lib.rs"), Ok(root.join("lib.rs")));
        assert_eq!(code("src/new.rs"), Ok(root.join("src/new.rs")));
        for outside in ["escape/key", "escape/missing.rs"] {
            assert_eq!(
                code(outside).map_err(|error| error.code),
                Err(ErrorCode::Forbidden),
                "{outside}"
            );
        }
        std::fs::remove_dir_all(&base)
    }
}
