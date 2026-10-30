use futures::channel::mpsc::{unbounded, UnboundedReceiver};
use futures::StreamExt as _;
use gpui_kit::{Context, Task};
use krad_chat::{start_chat, ChatBackend, ChatError, LiveChat, LiveEvent, LiveSession, Runner};
use krad_domain::{parse_wsl_path, AgentPreset};
use krad_session::find_program;
use std::fmt;
use std::path::{Path, PathBuf};

use super::super::terminal_card::home_dir;
use super::super::EnvironmentShell;

const NO_PROGRAM_TEXT: &str = "is not on PATH";
const NOT_IN_WSL_TEXT: &str = "chats do not run in WSL folders yet";
const WAITING_TEXT: &str = "opencode is still starting";
const NO_CONFIG_FOLDER_TEXT: &str = "Krad ADE has no config folder to keep the chat in";
const NO_CHAT_TEXT: &str = "has no chat";

#[derive(Debug)]
pub(in crate::environment_shell) enum StartError {
    NoProgram(AgentPreset),
    NoChat(AgentPreset),
    NotInWsl(AgentPreset),
    Waiting,
    Opencode(String),
    Gateway(String),
    NoConfigFolder,
    Chat(ChatError),
}

impl fmt::Display for StartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProgram(preset) => {
                write!(formatter, "{} {NO_PROGRAM_TEXT}", preset.display_name())
            }
            Self::NoChat(preset) => {
                write!(formatter, "{} {NO_CHAT_TEXT}", preset.display_name())
            }
            Self::NotInWsl(preset) => {
                write!(formatter, "{} {NOT_IN_WSL_TEXT}", preset.display_name())
            }
            Self::Waiting => formatter.write_str(WAITING_TEXT),
            Self::Opencode(error) | Self::Gateway(error) => formatter.write_str(error),
            Self::NoConfigFolder => formatter.write_str(NO_CONFIG_FOLDER_TEXT),
            Self::Chat(error) => error.fmt(formatter),
        }
    }
}

impl From<ChatError> for StartError {
    fn from(error: ChatError) -> Self {
        Self::Chat(error)
    }
}

pub(in crate::environment_shell) struct LiveProcess {
    pub(in crate::environment_shell) chat: LiveChat,
    _events: Task<()>,
    pub(in crate::environment_shell) confirmed: bool,
}

impl LiveProcess {
    pub(in crate::environment_shell) fn spawn(
        backend: ChatBackend,
        session: &LiveSession,
        cx: &mut Context<EnvironmentShell>,
        apply: impl FnMut(&mut EnvironmentShell, LiveEvent, &mut Context<EnvironmentShell>) + 'static,
    ) -> Result<Self, ChatError> {
        let (sender, receiver) = unbounded();
        let chat = start_chat(backend, session, move |event| {
            sender.unbounded_send(event).unwrap_or_default();
        })?;
        Ok(Self {
            chat,
            _events: EnvironmentShell::listen(receiver, cx, apply),
            confirmed: false,
        })
    }

    pub(in crate::environment_shell) fn alive(&self) -> Option<&LiveChat> {
        Some(&self.chat).filter(|chat| chat.is_alive())
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn listen<E: 'static>(
        mut receiver: UnboundedReceiver<E>,
        cx: &mut Context<Self>,
        mut apply: impl FnMut(&mut Self, E, &mut Context<Self>) + 'static,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Some(event) = receiver.next().await {
                let applied = this.update(cx, |shell, cx| {
                    apply(shell, event, cx);
                    cx.notify();
                });
                if applied.is_err() {
                    break;
                }
            }
        })
    }

    pub(in crate::environment_shell) fn harness_runner(
        &self,
        preset: AgentPreset,
        root: Option<&Path>,
    ) -> Result<(Runner, PathBuf), StartError> {
        let program = self
            .settings
            .launch_line(preset)
            .and_then(|line| line.split_whitespace().next().map(str::to_owned))
            .ok_or(StartError::NoProgram(preset))?;
        let in_wsl = root.and_then(|root| parse_wsl_path(root).map(|inside| (root, inside)));
        if let Some((root, inside)) = in_wsl {
            let runner = Runner::Wsl {
                distro: inside.distro,
                program,
            };
            return Ok((runner, root.to_path_buf()));
        }
        let runner = find_program(&program)
            .map(Runner::Native)
            .ok_or(StartError::NoProgram(preset))?;
        let cwd = root
            .map(Path::to_path_buf)
            .or_else(|| home_dir().map(PathBuf::from))
            .unwrap_or_default();
        Ok((runner, cwd))
    }
}
