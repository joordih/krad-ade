mod dispatch;

use krad_extension_api::protocol::{
    AppEventParams, CallToolParams, CallToolResult, ConnectMachineParams, ExecuteCommandParams,
    InitializeParams, ListMachinesParams, Machine, MachineChannel, RowsParams, RowsResult,
    SettingsValues, ViewEventParams, ViewRef, WorkspaceEvent,
};
use krad_extension_api::view::Node;
use krad_extension_api::{CommandId, HostKindId, ToolName, ViewId};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use crate::app::{App, APP_REQUEST_TIMEOUT};
use crate::error::{HandlerError, SdkError};

pub type Handler<P, R> = Box<dyn Fn(&App, P) -> Result<R, HandlerError> + Send + Sync>;
pub type Hook<P> = Box<dyn Fn(&App, P) + Send + Sync>;

#[derive(Default)]
pub(crate) struct Handlers {
    initialize: Option<Hook<InitializeParams>>,
    commands: HashMap<CommandId, Handler<ExecuteCommandParams, ()>>,
    renders: HashMap<ViewId, Handler<ViewRef, Node>>,
    rows: HashMap<ViewId, Handler<RowsParams, RowsResult>>,
    view_events: HashMap<ViewId, Hook<ViewEventParams>>,
    view_closed: HashMap<ViewId, Hook<ViewRef>>,
    tools: HashMap<ToolName, Handler<CallToolParams, CallToolResult>>,
    machines: HashMap<HostKindId, Handler<ListMachinesParams, Vec<Machine>>>,
    connections: HashMap<HostKindId, Handler<ConnectMachineParams, MachineChannel>>,
    workspace_events: Option<Hook<WorkspaceEvent>>,
    app_events: Option<Hook<AppEventParams>>,
    settings_changed: Option<Hook<SettingsValues>>,
}

pub struct Extension {
    handlers: Handlers,
    timeout: Duration,
}

impl Default for Extension {
    fn default() -> Self {
        Self::new()
    }
}

impl Extension {
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: Handlers::default(),
            timeout: APP_REQUEST_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn on_initialize(
        mut self,
        hook: impl Fn(&App, InitializeParams) + Send + Sync + 'static,
    ) -> Self {
        self.handlers.initialize = Some(Box::new(hook));
        self
    }

    #[must_use]
    pub fn on_command(
        mut self,
        command: CommandId,
        handler: impl Fn(&App, ExecuteCommandParams) -> Result<(), HandlerError> + Send + Sync + 'static,
    ) -> Self {
        self.handlers.commands.insert(command, Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_render(
        mut self,
        view: ViewId,
        handler: impl Fn(&App, ViewRef) -> Result<Node, HandlerError> + Send + Sync + 'static,
    ) -> Self {
        self.handlers.renders.insert(view, Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_rows(
        mut self,
        view: ViewId,
        handler: impl Fn(&App, RowsParams) -> Result<RowsResult, HandlerError> + Send + Sync + 'static,
    ) -> Self {
        self.handlers.rows.insert(view, Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_view_event(
        mut self,
        view: ViewId,
        hook: impl Fn(&App, ViewEventParams) + Send + Sync + 'static,
    ) -> Self {
        self.handlers.view_events.insert(view, Box::new(hook));
        self
    }

    #[must_use]
    pub fn on_view_closed(
        mut self,
        view: ViewId,
        hook: impl Fn(&App, ViewRef) + Send + Sync + 'static,
    ) -> Self {
        self.handlers.view_closed.insert(view, Box::new(hook));
        self
    }

    #[must_use]
    pub fn on_popover(
        self,
        view: ViewId,
        opened: impl Fn(&App, ViewRef) -> Result<Node, HandlerError> + Send + Sync + 'static,
        closed: impl Fn(&App, ViewRef) + Send + Sync + 'static,
    ) -> Self {
        self.on_render(view.clone(), opened)
            .on_view_closed(view, closed)
    }

    #[must_use]
    pub fn on_tool(
        mut self,
        tool: ToolName,
        handler: impl Fn(&App, CallToolParams) -> Result<CallToolResult, HandlerError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.handlers.tools.insert(tool, Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_list_machines(
        mut self,
        kind: HostKindId,
        handler: impl Fn(&App, ListMachinesParams) -> Result<Vec<Machine>, HandlerError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.handlers.machines.insert(kind, Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_connect_machine(
        mut self,
        kind: HostKindId,
        handler: impl Fn(&App, ConnectMachineParams) -> Result<MachineChannel, HandlerError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.handlers.connections.insert(kind, Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_workspace_event(
        mut self,
        hook: impl Fn(&App, WorkspaceEvent) + Send + Sync + 'static,
    ) -> Self {
        self.handlers.workspace_events = Some(Box::new(hook));
        self
    }

    #[must_use]
    pub fn on_app_event(
        mut self,
        hook: impl Fn(&App, AppEventParams) + Send + Sync + 'static,
    ) -> Self {
        self.handlers.app_events = Some(Box::new(hook));
        self
    }

    #[must_use]
    pub fn on_settings_changed(
        mut self,
        hook: impl Fn(&App, SettingsValues) + Send + Sync + 'static,
    ) -> Self {
        self.handlers.settings_changed = Some(Box::new(hook));
        self
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when stdin or stdout fail before the app says `exit`.
    pub fn run(self) -> Result<(), SdkError> {
        self.run_over(BufReader::new(std::io::stdin()), std::io::stdout())
    }

    /// # Errors
    ///
    /// Returns [`SdkError`] when the pipes fail before the app says `exit`.
    pub fn run_over(
        self,
        reader: impl BufRead,
        writer: impl Write + Send + 'static,
    ) -> Result<(), SdkError> {
        let app = App::new(Box::new(writer), self.timeout);
        dispatch::serve(self.handlers, &app, reader)
    }
}
