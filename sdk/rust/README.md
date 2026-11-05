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
