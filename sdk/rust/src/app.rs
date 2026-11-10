use krad_extension_api::protocol::{
    Answer, AppEventKind, CloseView, Confirm, ConfirmParams, Empty, GetSecret, GetSecretParams,
    GetSettings, InputBox, InputBoxParams, Log, LogLevel, LogParams, Notification, OpenFile,
    OpenFileParams, OpenTerminal, OpenTerminalParams, OpenView, OpenViewParams, Party, PatchView,
    PatchViewParams, Pick, PickParams, Progress, ProgressParams, Request, RunCommand,
    RunCommandParams, Secret, SendTerminal, SendTerminalParams, SetSecret, SetSecretParams,
    SetTitlebarItem, SetTitlebarItemParams, SettingsValues, SubscribeEvents, SubscribeEventsParams,
    Toast, ToastParams, ViewRef, WorkspaceContext, WorkspaceContextResult,
};
use krad_extension_api::view::{Patch, Tone};
use krad_extension_api::{SecretName, TerminalHandle};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::error::SdkError;
use crate::rpc::{write_message, Message, ResponseError};

pub const APP_REQUEST_TIMEOUT: Duration = Duration::from_mins(10);

type Waiting = Sender<Result<Value, SdkError>>;

struct Shared {
    writer: Mutex<Box<dyn Write + Send>>,
    pending: Mutex<HashMap<i64, Waiting>>,
    next_id: AtomicI64,
    timeout: Duration,
}

#[derive(Clone)]
pub struct App {
    shared: Arc<Shared>,
}

impl App {
    pub(crate) fn new(writer: Box<dyn Write + Send>, timeout: Duration) -> Self {
        Self {
            shared: Arc::new(Shared {
                writer: Mutex::new(writer),
                pending: Mutex::new(HashMap::new()),
                next_id: AtomicI64::new(1),
                timeout,
            }),
        }
    }

    pub(crate) fn send(&self, message: &Message) -> Result<(), SdkError> {
        let mut writer = self
            .shared
            .writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        write_message(&mut *writer, message)
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<i64, Waiting>> {
        self.shared
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn resolve(&self, id: &Value, outcome: Result<Value, ResponseError>) {
        let Some(waiting) = id.as_i64().and_then(|id| self.pending().remove(&id)) else {
            return;
        };
        let outcome = outcome.map_err(|error| SdkError::Refused {
            code: error.code,
            message: error.message,
        });
        waiting.send(outcome).unwrap_or_default();
    }

    pub(crate) fn fail_waiting(&self) {
        for (_, waiting) in self.pending().drain() {
            waiting.send(Err(SdkError::Stopped)).unwrap_or_default();
        }
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the method is answered by the extension, the app stops, refuses,
    /// answers late or with data of another shape.
    pub fn request<R: Request>(&self, params: &R::Params) -> Result<R::Result, SdkError> {
        if R::ANSWERED_BY != Party::App {
            return Err(SdkError::WrongDirection(R::METHOD));
        }
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        let (waiting, answer) = channel();
        self.pending().insert(id, waiting);
        let sent = self.send(&Message::Request {
            id: Value::from(id),
            method: R::METHOD.to_owned(),
            params: serde_json::to_value(params)?,
        });
        if let Err(error) = sent {
            self.pending().remove(&id);
            return Err(error);
        }
        let value = match answer.recv_timeout(self.shared.timeout) {
            Ok(outcome) => outcome?,
            Err(RecvTimeoutError::Timeout) => {
                self.pending().remove(&id);
                return Err(SdkError::Timeout);
            }
            Err(RecvTimeoutError::Disconnected) => return Err(SdkError::Stopped),
        };
        Ok(serde_json::from_value(value)?)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the notification is meant for the extension or cannot be sent.
    pub fn notify<N: Notification>(&self, params: &N::Params) -> Result<(), SdkError> {
        if N::RECEIVED_BY != Party::App {
            return Err(SdkError::WrongDirection(N::METHOD));
        }
        self.send(&Message::Notification {
            method: N::METHOD.to_owned(),
            params: serde_json::to_value(params)?,
        })
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn toast(&self, message: impl Into<String>, tone: Tone) -> Result<(), SdkError> {
        self.notify::<Toast>(&ToastParams {
            message: message.into(),
            tone,
        })
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn log(&self, level: LogLevel, message: impl Into<String>) -> Result<(), SdkError> {
        self.notify::<Log>(&LogParams {
            level,
            message: message.into(),
        })
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn confirm(&self, params: &ConfirmParams) -> Result<Answer, SdkError> {
        self.request::<Confirm>(params)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn pick(&self, params: &PickParams) -> Result<Option<Vec<String>>, SdkError> {
        self.request::<Pick>(params)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn input_box(&self, params: &InputBoxParams) -> Result<Option<String>, SdkError> {
        self.request::<InputBox>(params)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn progress(&self, params: &ProgressParams) -> Result<(), SdkError> {
        self.notify::<Progress>(params)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached or refuses to open the terminal.
    pub fn open_terminal(&self, params: &OpenTerminalParams) -> Result<TerminalHandle, SdkError> {
        self.request::<OpenTerminal>(params)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn send_terminal(
        &self,
        terminal: TerminalHandle,
        text: impl Into<String>,
    ) -> Result<(), SdkError> {
        self.notify::<SendTerminal>(&SendTerminalParams {
            terminal,
            text: text.into(),
        })
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached or refuses the file.
    pub fn open_file(&self, params: &OpenFileParams) -> Result<(), SdkError> {
        self.request::<OpenFile>(params).map(drop)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn workspace_context(&self) -> Result<WorkspaceContextResult, SdkError> {
        self.request::<WorkspaceContext>(&Empty {})
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn subscribe_events(
        &self,
        kinds: impl IntoIterator<Item = AppEventKind>,
    ) -> Result<(), SdkError> {
        let params = SubscribeEventsParams {
            kinds: kinds.into_iter().collect(),
        };
        self.request::<SubscribeEvents>(&params).map(drop)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn settings(&self) -> Result<SettingsValues, SdkError> {
        self.request::<GetSettings>(&Empty {})
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached or the keyring fails.
    pub fn secret(&self, name: SecretName) -> Result<Option<Secret>, SdkError> {
        self.request::<GetSecret>(&GetSecretParams { name })
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached or the keyring fails.
    pub fn set_secret(&self, name: SecretName, value: Option<Secret>) -> Result<(), SdkError> {
        self.request::<SetSecret>(&SetSecretParams { name, value })
            .map(drop)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached or the command fails.
    pub fn run_command(&self, params: &RunCommandParams) -> Result<(), SdkError> {
        self.request::<RunCommand>(params).map(drop)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached or does not know the view.
    pub fn open_view(&self, params: &OpenViewParams) -> Result<(), SdkError> {
        self.request::<OpenView>(params).map(drop)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn close_view(&self, view: &ViewRef) -> Result<(), SdkError> {
        self.request::<CloseView>(view).map(drop)
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn patch_view(&self, view: ViewRef, patches: Vec<Patch>) -> Result<(), SdkError> {
        self.notify::<PatchView>(&PatchViewParams { view, patches })
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the app cannot be reached.
    pub fn set_titlebar_item(&self, params: &SetTitlebarItemParams) -> Result<(), SdkError> {
        self.notify::<SetTitlebarItem>(params)
    }
}
