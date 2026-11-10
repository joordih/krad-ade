use gpui_kit::Context;
use krad_agents::{ErrorCode, KraddieChatInfo, Response};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    EnvironmentName, EnvironmentView, GroupId, KraddieId, Session, SessionId, SessionSpec,
    TerminalId, WorkerSurface,
};

use super::super::session_templates::ChatChoices;
use super::super::terminal_lifecycle::NodeGroup;
use super::super::EnvironmentShell;

const CHAT_NAME_SUFFIX: &str = " chat";
const NOT_A_CHAT_KRADDIE: &str = "This Kraddie's agent cannot open a chat; pick another in Tuning";
const NO_ENVIRONMENT: &str = "No environment is open for the chat";
const NO_KRADDIE: &str = "That Kraddie is no longer here";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OpenedChat {
    pub(super) session: SessionId,
    pub(super) terminal: TerminalId,
}

impl EnvironmentShell {
    fn kraddie_chat_group(&mut self, id: &KraddieId, cx: &mut Context<Self>) -> Option<GroupId> {
        let env = self.active.clone();
        let own = self
            .kraddies
            .entry(id)
            .and_then(|entry| entry.kraddie.workspace.clone())
            .filter(|place| place.environment == env);
        let Some(place) = own else {
            return self.active_group().map(|group| group.id);
        };
        let known = self.envs.get(&env).and_then(|live| {
            live.data
                .groups
                .iter()
                .find(|group| group.root == place.repo)
                .map(|group| group.id)
        });
        known.or_else(|| self.add_group_to(&env, place.repo, None, cx))
    }

    pub(super) fn open_kraddie_chat(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) -> Result<OpenedChat, &'static str> {
        let entry = self.kraddies.entry(id).ok_or(NO_KRADDIE)?;
        let kraddie = entry.kraddie.clone();
        if kraddie.harness.worker_surface() != WorkerSurface::Chat {
            return Err(NOT_A_CHAT_KRADDIE);
        }
        let env = self.active.clone();
        let group = self.kraddie_chat_group(id, cx);
        let spec = SessionSpec {
            name: format!("{}{CHAT_NAME_SUFFIX}", kraddie.name),
        };
        let session = self
            .envs
            .get_mut(&env)
            .map(|live| live.data.new_kraddie_session(group, id.clone(), spec))
            .ok_or(NO_ENVIRONMENT)?;
        self.capture_session_baseline(cx);
        let terminal = self
            .spawn_chat_as(
                &env,
                NodeGroup::Exactly(group),
                kraddie.harness,
                LiveSession::Start(new_session_id()),
                cx,
            )
            .ok_or(NO_ENVIRONMENT)?;
        let settings = &kraddie.settings;
        let model = settings.session_model.clone().or(kraddie.model.clone());
        if let Some(live) = self.envs.get_mut(&env) {
            if model.is_some() || settings.effort.is_some() {
                live.waiting_choices.insert(
                    terminal,
                    ChatChoices {
                        model,
                        mcp_servers: None,
                        effort: settings.effort,
                    },
                );
            }
            if let Some(mode) = settings.permissions {
                live.waiting_modes.insert(terminal, mode);
            }
        }
        tracing::info!(kraddie = %id, session = session.as_u64(), "kraddie chat opened");
        self.set_view(EnvironmentView::Sessions, cx);
        self.store();
        cx.notify();
        Ok(OpenedChat { session, terminal })
    }

    pub(super) fn start_kraddie_chat(&mut self, id: &KraddieId, cx: &mut Context<Self>) {
        if let Err(reason) = self.open_kraddie_chat(id, cx) {
            self.toast(reason, cx);
        }
    }

    pub(super) fn kraddie_chat_from_bus(&mut self, name: &str, cx: &mut Context<Self>) -> Response {
        let id = match self.kraddie_by_name(name) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match self.open_kraddie_chat(&id, cx) {
            Ok(opened) => Response::KraddieChatOpened(KraddieChatInfo {
                kraddie: id.to_string(),
                session: opened.session.as_u64(),
                terminal: opened.terminal.as_u64(),
            }),
            Err(reason) => Response::error(ErrorCode::Unavailable, reason),
        }
    }

    pub(super) fn kraddie_chats<'a>(
        &'a self,
        id: &'a KraddieId,
    ) -> Vec<(EnvironmentName, &'a Session)> {
        self.envs
            .iter()
            .flat_map(|(env, live)| {
                live.data
                    .kraddie_sessions(id)
                    .map(move |session| (env.clone(), session))
            })
            .collect()
    }

    pub(super) fn reopen_kraddie_chat(
        &mut self,
        env: &EnvironmentName,
        session: SessionId,
        cx: &mut Context<Self>,
    ) {
        if *env != self.active {
            return;
        }
        let shown = self.envs.get_mut(env).is_some_and(|live| {
            let selected = live.data.select_session(session);
            if selected {
                live.data.view = EnvironmentView::Sessions;
            }
            selected
        });
        if shown {
            self.store();
            cx.notify();
        }
    }
}
