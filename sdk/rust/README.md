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

See `examples/hello` for a command and a sidebar view.

Licensed under MIT or Apache-2.0, at your option.
