use crate::rpc::{METHOD_NOT_FOUND, REQUEST_FAILED};
use crate::{App, Extension, HandlerError, Message, ResponseError, SdkError};
use krad_extension_api::protocol::{
    Answer, ConfirmParams, Feature, RenderView, SetTitlebarItemParams,
};
use krad_extension_api::view::{Element, Node, TextStyle, Tone, ViewEvent};
use krad_extension_api::Icon;
use serde_json::{json, Value};
use std::io::{pipe, BufReader, PipeReader, PipeWriter};
use std::sync::mpsc::{channel, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const WAIT: Duration = Duration::from_secs(5);

struct FakeApp {
    from_extension: BufReader<PipeReader>,
    to_extension: PipeWriter,
    running: JoinHandle<Result<(), SdkError>>,
}

impl FakeApp {
    fn start(extension: Extension) -> Result<Self, SdkError> {
        let (extension_reads, app_writes) = pipe()?;
        let (app_reads, extension_writes) = pipe()?;
        let running = thread::spawn(move || {
            extension.run_over(BufReader::new(extension_reads), extension_writes)
        });
        Ok(Self {
            from_extension: BufReader::new(app_reads),
            to_extension: app_writes,
            running,
        })
    }

    fn send(&mut self, message: &Message) -> Result<(), SdkError> {
        crate::write_message(&mut self.to_extension, message)
    }

    fn request(&mut self, id: i64, method: &str, params: Value) -> Result<(), SdkError> {
        self.send(&Message::Request {
            id: json!(id),
            method: method.to_owned(),
            params,
        })
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), SdkError> {
        self.send(&Message::Notification {
            method: method.to_owned(),
            params,
        })
    }

    fn read(&mut self) -> Result<Message, Box<dyn std::error::Error>> {
        crate::read_message(&mut self.from_extension)?.ok_or_else(|| "the extension closed".into())
    }

    fn exit(mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.notify("exit", json!({}))?;
        self.running
            .join()
            .map_err(|_| "the extension panicked")??;
        Ok(())
    }
}

fn initialize_params() -> Value {
    json!({
        "app": {"name": "Krad ADE", "version": "0.2.8"},
        "api": "1.0",
        "placement": "app",
        "extension_folder": "/extensions/hello",
        "features": ["views", "commands"]
    })
}

fn result_of(message: Message) -> Result<Value, ResponseError> {
    match message {
        Message::Response { outcome, .. } => outcome,
        other => Err(ResponseError {
            code: 0,
            message: format!("expected a response, got {other:?}"),
        }),
    }
}

#[test]
fn initialize_offers_the_features_of_the_registered_handlers() -> TestResult {
    let (seen, told) = channel::<String>();
    let extension = Extension::new()
        .on_initialize(move |_, params| seen.send(params.app.name).unwrap_or_default())
        .on_command("hello".parse()?, |_, _| Ok(()));
    let mut app = FakeApp::start(extension)?;
    app.request(1, "initialize", initialize_params())?;
    let result = result_of(app.read()?)?;
    let features: Vec<Feature> = serde_json::from_value(result["features"].clone())?;
    assert!(features.contains(&Feature::Commands));
    assert!(!features.contains(&Feature::Views));
    assert_eq!(told.recv_timeout(WAIT)?, "Krad ADE");
    app.exit()
}

#[test]
fn a_command_can_notify_the_app_before_it_answers() -> TestResult {
    let extension = Extension::new().on_command("hello".parse()?, |app, _| {
        app.toast("Hello from Rust", Tone::Success)?;
        Ok(())
    });
    let mut app = FakeApp::start(extension)?;
    app.request(2, "command/execute", json!({"command": "hello"}))?;
    let Message::Notification { method, params } = app.read()? else {
        return Err("expected the toast first".into());
    };
    assert_eq!(method, "window/toast");
    assert_eq!(params["message"], "Hello from Rust");
    assert_eq!(result_of(app.read()?)?, json!({}));
    app.exit()
}

#[test]
fn a_handler_waits_for_the_apps_answer_to_its_own_request() -> TestResult {
    let (answers, answered) = channel::<Answer>();
    let extension = Extension::new().on_command("remove".parse()?, move |app, _| {
        let answer = app.confirm(&ConfirmParams {
            title: "Remove web?".to_owned(),
            detail: None,
            confirm: "Remove".to_owned(),
            tone: Tone::Error,
        })?;
        answers
            .send(answer)
            .map_err(|error| HandlerError::new(error.to_string()))
    });
    let mut app = FakeApp::start(extension)?;
    app.request(3, "command/execute", json!({"command": "remove"}))?;
    let Message::Request { id, method, .. } = app.read()? else {
        return Err("expected the confirm request".into());
    };
    assert_eq!(method, "window/confirm");
    app.send(&Message::Response {
        id,
        outcome: Ok(json!("confirmed")),
    })?;
    assert_eq!(answered.recv_timeout(WAIT)?, Answer::Confirmed);
    assert_eq!(result_of(app.read()?)?, json!({}));
    app.exit()
}

#[test]
fn unknown_methods_and_missing_handlers_are_refused() -> TestResult {
    let mut app = FakeApp::start(Extension::new())?;
    app.request(4, "window/teleport", json!({}))?;
    assert!(matches!(
        result_of(app.read()?),
        Err(ResponseError {
            code: METHOD_NOT_FOUND,
            ..
        })
    ));
    app.request(5, "command/execute", json!({"command": "nobody"}))?;
    assert!(matches!(
        result_of(app.read()?),
        Err(ResponseError {
            code: REQUEST_FAILED,
            ..
        })
    ));
    app.exit()
}

#[test]
fn view_events_reach_the_hook_of_their_view() -> TestResult {
    let (events, received): (Sender<ViewEvent>, _) = channel();
    let extension = Extension::new().on_view_event("hello".parse()?, move |_, params| {
        events.send(params.event).unwrap_or_default();
    });
    let mut app = FakeApp::start(extension)?;
    app.notify(
        "view/event",
        json!({"view": "hello", "event": {"kind": "click", "action": "wave"}}),
    )?;
    let event = received.recv_timeout(WAIT)?;
    assert_eq!(
        event,
        ViewEvent::Click {
            action: "wave".parse()?
        }
    );
    app.exit()
}

#[test]
fn the_extension_cannot_ask_the_app_what_only_it_answers() -> TestResult {
    let app = App::new(Box::new(Vec::new()), WAIT);
    let view = serde_json::from_value(json!({"view": "hello"}))?;
    assert!(matches!(
        app.request::<RenderView>(&view),
        Err(SdkError::WrongDirection("view/render"))
    ));
    Ok(())
}

#[test]
fn a_popover_lights_its_button_while_open_and_hears_when_it_closes() -> TestResult {
    let (closings, closed) = channel::<String>();
    let item: krad_extension_api::TitlebarItemId = "now-playing".parse()?;
    let track: krad_extension_api::NodeKey = "track".parse()?;
    let extension = Extension::new().on_popover(
        "now-playing".parse()?,
        move |app, _| {
            app.set_titlebar_item(&SetTitlebarItemParams {
                item: item.clone(),
                icon: Some(Icon::Stop),
                title: Some("Pause".to_owned()),
                active: Some(true),
                hidden: None,
            })?;
            Ok(Node {
                key: track.clone(),
                element: Element::Text {
                    text: "Track one".to_owned(),
                    style: TextStyle::default(),
                },
            })
        },
        move |_, view| closings.send(view.view.to_string()).unwrap_or_default(),
    );
    let mut app = FakeApp::start(extension)?;
    app.request(6, "view/render", json!({"view": "now-playing"}))?;
    let Message::Notification { method, params } = app.read()? else {
        return Err("expected the titlebar state first".into());
    };
    assert_eq!(method, "titlebar/set");
    assert_eq!(
        params,
        json!({"item": "now-playing", "icon": "stop", "title": "Pause", "active": true})
    );
    assert_eq!(result_of(app.read()?)?["type"], "text");
    app.notify("view/closed", json!({"view": "now-playing"}))?;
    assert_eq!(closed.recv_timeout(WAIT)?, "now-playing");
    app.exit()
}
