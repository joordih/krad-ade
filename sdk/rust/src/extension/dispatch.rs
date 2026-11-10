use krad_extension_api::protocol::{
    AppEventNotification, CallTool, ConnectMachine, Empty, ExecuteCommand, Exit, Feature,
    Initialize, InitializeResult, Initialized, ListMachines, Notification, RenderView, Request,
    Rows, SettingsChanged, Shutdown, ViewClosed, ViewEventNotification, WorkspaceEventNotification,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt::Display;
use std::io::BufRead;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

use super::{Handler, Handlers};
use crate::app::App;
use crate::error::{HandlerError, SdkError};
use crate::rpc::{
    read_message, Message, ResponseError, INVALID_PARAMS, METHOD_NOT_FOUND, REQUEST_FAILED,
};

const ALWAYS_OFFERED: [Feature; 6] = [
    Feature::Window,
    Feature::Terminals,
    Feature::Files,
    Feature::Workspace,
    Feature::Settings,
    Feature::Secrets,
];
const NO_HANDLER: &str = "no handler for";

pub(super) fn serve(
    handlers: Handlers,
    app: &App,
    mut reader: impl BufRead,
) -> Result<(), SdkError> {
    let handlers = Arc::new(handlers);
    let (notices, inbox) = channel::<(String, Value)>();
    let worker = {
        let (handlers, app) = (Arc::clone(&handlers), app.clone());
        thread::spawn(move || handle_notices(&handlers, &app, &inbox))
    };
    let outcome = loop {
        let message = match read_message(&mut reader) {
            Ok(Some(message)) => message,
            Ok(None) => break Ok(()),
            Err(error) => break Err(error),
        };
        match message {
            Message::Response { id, outcome } => app.resolve(&id, outcome),
            Message::Request { id, method, params } => {
                let (handlers, app) = (Arc::clone(&handlers), app.clone());
                thread::spawn(move || {
                    let outcome = answer(&handlers, &app, &method, params);
                    app.send(&Message::Response { id, outcome })
                        .unwrap_or_default();
                });
            }
            Message::Notification { method, params } => {
                if method == Exit::METHOD {
                    break Ok(());
                }
                if notices.send((method, params)).is_err() {
                    break Err(SdkError::Stopped);
                }
            }
        }
    };
    app.fail_waiting();
    drop(notices);
    worker.join().map_err(|_| SdkError::Stopped)?;
    outcome
}

fn refused(code: i64, message: impl Display) -> ResponseError {
    ResponseError {
        code,
        message: message.to_string(),
    }
}

fn call<R: Request>(
    params: Value,
    run: impl FnOnce(R::Params) -> Result<R::Result, HandlerError>,
) -> Result<Value, ResponseError> {
    let params: R::Params =
        serde_json::from_value(params).map_err(|error| refused(INVALID_PARAMS, error))?;
    let result = run(params).map_err(|error| refused(REQUEST_FAILED, error))?;
    serde_json::to_value(result).map_err(|error| refused(REQUEST_FAILED, error))
}

fn handled<K: Display + Eq + std::hash::Hash, P, R>(
    handlers: &std::collections::HashMap<K, Handler<P, R>>,
    key: &K,
    app: &App,
    params: P,
) -> Result<R, HandlerError> {
    let handler = handlers
        .get(key)
        .ok_or_else(|| HandlerError::new(format!("{NO_HANDLER} {key}")))?;
    handler(app, params)
}

fn features(handlers: &Handlers) -> BTreeSet<Feature> {
    let offered = [
        (!handlers.commands.is_empty(), Feature::Commands),
        (!handlers.renders.is_empty(), Feature::Views),
        (!handlers.tools.is_empty(), Feature::Tools),
        (!handlers.machines.is_empty(), Feature::HostKinds),
        (handlers.app_events.is_some(), Feature::Events),
    ];
    offered
        .into_iter()
        .filter_map(|(present, feature)| present.then_some(feature))
        .chain(ALWAYS_OFFERED)
        .collect()
}

fn answer(
    handlers: &Handlers,
    app: &App,
    method: &str,
    params: Value,
) -> Result<Value, ResponseError> {
    match method {
        Initialize::METHOD => call::<Initialize>(params, |params| {
            if let Some(hook) = &handlers.initialize {
                hook(app, params);
            }
            Ok(InitializeResult {
                features: features(handlers),
            })
        }),
        Shutdown::METHOD => call::<Shutdown>(params, |_| Ok(Empty {})),
        ExecuteCommand::METHOD => call::<ExecuteCommand>(params, |params| {
            let command = params.command.clone();
            handled(&handlers.commands, &command, app, params).map(|()| Empty {})
        }),
        RenderView::METHOD => call::<RenderView>(params, |params| {
            let view = params.view.clone();
            handled(&handlers.renders, &view, app, params)
        }),
        Rows::METHOD => call::<Rows>(params, |params| {
            let view = params.view.view.clone();
            handled(&handlers.rows, &view, app, params)
        }),
        CallTool::METHOD => call::<CallTool>(params, |params| {
            let tool = params.tool.clone();
            handled(&handlers.tools, &tool, app, params)
        }),
        ListMachines::METHOD => call::<ListMachines>(params, |params| {
            let kind = params.kind.clone();
            handled(&handlers.machines, &kind, app, params)
        }),
        ConnectMachine::METHOD => call::<ConnectMachine>(params, |params| {
            let kind = params.kind.clone();
            handled(&handlers.connections, &kind, app, params)
        }),
        unknown => Err(refused(METHOD_NOT_FOUND, unknown)),
    }
}

fn notice<N: Notification>(params: Value) -> Option<N::Params> {
    serde_json::from_value(params).ok()
}

fn handle_notices(handlers: &Handlers, app: &App, inbox: &Receiver<(String, Value)>) {
    for (method, params) in inbox {
        dispatch_notice(handlers, app, &method, params);
    }
}

fn dispatch_notice(handlers: &Handlers, app: &App, method: &str, params: Value) {
    match method {
        Initialized::METHOD => {}
        ViewEventNotification::METHOD => {
            if let Some(params) = notice::<ViewEventNotification>(params) {
                if let Some(hook) = handlers.view_events.get(&params.view.view) {
                    hook(app, params);
                }
            }
        }
        ViewClosed::METHOD => {
            if let Some(view) = notice::<ViewClosed>(params) {
                if let Some(hook) = handlers.view_closed.get(&view.view) {
                    hook(app, view);
                }
            }
        }
        WorkspaceEventNotification::METHOD => {
            if let (Some(event), Some(hook)) = (
                notice::<WorkspaceEventNotification>(params),
                &handlers.workspace_events,
            ) {
                hook(app, event);
            }
        }
        AppEventNotification::METHOD => {
            if let (Some(event), Some(hook)) =
                (notice::<AppEventNotification>(params), &handlers.app_events)
            {
                hook(app, event);
            }
        }
        SettingsChanged::METHOD => {
            if let (Some(values), Some(hook)) = (
                notice::<SettingsChanged>(params),
                &handlers.settings_changed,
            ) {
                hook(app, values);
            }
        }
        _ => {}
    }
}
