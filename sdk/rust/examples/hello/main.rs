use krad_extension_sdk::api::protocol::ViewRef;
use krad_extension_sdk::api::view::{
    Align, ButtonVariant, Direction, Element, Enabled, Justify, Node, Patch, Style, TextStyle,
    Tone, ViewEvent, Wrap,
};
use krad_extension_sdk::api::{IdError, NodeKey};
use krad_extension_sdk::{App, Extension, HandlerError};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const HELLO: &str = "hello";
const ROOT: &str = "root";
const GREETING: &str = "greeting";
const WAVE: &str = "wave";
const FIRST_GREETING: &str = "Nobody has waved yet";
const WAVE_LABEL: &str = "Wave";
const TOAST: &str = "Hello from a Rust extension";

fn greeting(waves: u32) -> Result<Node, IdError> {
    let text = if waves == 0 {
        FIRST_GREETING.to_owned()
    } else {
        format!("Waved {waves} times")
    };
    Ok(Node {
        key: GREETING.parse()?,
        element: Element::Text {
            text,
            style: TextStyle::default(),
        },
    })
}

fn view(waves: u32) -> Result<Node, IdError> {
    let button = Node {
        key: NodeKey::try_from(WAVE.to_owned())?,
        element: Element::Button {
            action: WAVE.parse()?,
            label: Some(WAVE_LABEL.to_owned()),
            icon: None,
            variant: ButtonVariant::default(),
            enabled: Enabled::default(),
            tooltip: None,
        },
    };
    Ok(Node {
        key: ROOT.parse()?,
        element: Element::Flex {
            direction: Direction::Column,
            align: Align::default(),
            justify: Justify::default(),
            wrap: Wrap::default(),
            style: Style::default(),
            children: vec![greeting(waves)?, button],
        },
    })
}

fn waved(app: &App, view_ref: ViewRef, waves: &AtomicU32) -> Result<(), HandlerError> {
    let count = waves.fetch_add(1, Ordering::Relaxed) + 1;
    let node = greeting(count).map_err(|error| HandlerError::new(error.to_string()))?;
    app.patch_view(view_ref, vec![Patch::Replace { node }])?;
    Ok(())
}

fn extension() -> Result<Extension, IdError> {
    let waves = Arc::new(AtomicU32::new(0));
    let rendered = Arc::clone(&waves);
    Ok(Extension::new()
        .on_command(HELLO.parse()?, |app, _| {
            app.toast(TOAST, Tone::Success)?;
            Ok(())
        })
        .on_render(HELLO.parse()?, move |_, _| {
            view(rendered.load(Ordering::Relaxed))
                .map_err(|error| HandlerError::new(error.to_string()))
        })
        .on_view_event(HELLO.parse()?, move |app, params| {
            if matches!(&params.event, ViewEvent::Click { action } if action.as_ref() == WAVE) {
                waved(app, params.view, &waves).unwrap_or_default();
            }
        }))
}

fn main() -> ExitCode {
    match extension().map(Extension::run) {
        Ok(Ok(())) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
