# krad-extension-sdk

Write [Krad ADE](https://github.com/joordih/krad-ade) extension programs in Rust.

The SDK speaks JSON-RPC 2.0 with LSP framing over stdin and stdout, answers `initialize` with the features your handlers cover, and gives you a typed `App` client for every request and notification the app answers.

```rust
use krad_extension_sdk::api::view::Tone;
use krad_extension_sdk::Extension;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Extension::new()
        .on_command("greet".parse()?, |app, _params| {
            app.toast("Hello from a Rust extension", Tone::Success)?;
            Ok(())
        })
        .run()?;
    Ok(())
}
```

The extension's `extension.toml` points at the built binary:

```toml
id = "hello"
name = "Hello"
version = "0.1.0"
api = "1.0"

[program]
placement = "app"

[[program.binary]]
targets = ["darwin_arm64"]
path = "bin/hello"

[[contributes.command]]
id = "greet"
title = "Hello: Greet"
```

A titlebar button next to the ADE/Editor switch opens a `popover` view (manifest `api = "1.1"`,
`[[contributes.view]]` with `zone = "popover"` and `[[contributes.titlebar_item]]` with
`id`, `title`, `icon`, `side` and `view` or `command`). `on_popover` answers its opening and hears
its closing, and `App::set_titlebar_item` changes the button's icon, tooltip, highlight or
visibility:

```rust
use krad_extension_sdk::api::protocol::SetTitlebarItemParams;

let item: krad_extension_sdk::api::TitlebarItemId = "now-playing".parse()?;
Extension::new().on_popover(
    "now-playing".parse()?,
    move |app, _view| {
        app.set_titlebar_item(&SetTitlebarItemParams {
            item: item.clone(),
            icon: None,
            title: None,
            active: Some(true),
            hidden: None,
        })?;
        Ok(now_playing())
    },
    |_app, _view| {},
);
```

## Sliders

`slider` builds a slider node (`api = "1.1"`). With `advancing` the app moves the value by itself at
`rate` units per second until `max`; the view receives `ViewEvent::SliderChange` while dragging and
`ViewEvent::SliderCommit` on release or a key press.

```rust
use krad_extension_sdk::api::view::{Node, SliderFormat, SliderLabels, ViewEvent};
use krad_extension_sdk::slider;

fn timeline(position: f64) -> Result<Node, krad_extension_sdk::api::IdError> {
    Ok(slider("timeline".parse()?, "seek".parse()?, position, 245.0)
        .format(SliderFormat::Duration)
        .labels(SliderLabels::ElapsedRemaining)
        .advancing(1.0)
        .into())
}

fn seek_to(event: &ViewEvent) -> Option<f64> {
    match event {
        ViewEvent::SliderCommit { value, .. } => Some(*value),
        _ => None,
    }
}
```

See `examples/hello` for a command and a sidebar view.

Licensed under MIT or Apache-2.0, at your option.
