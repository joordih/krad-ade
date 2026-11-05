# krad-extension-api

The protocol between [Krad ADE](https://github.com/joordih/krad-ade) and extension programs. It has no I/O; it only holds types:

- `manifest`: the `extension.toml` an extension ships: its program, permissions, activation and contributions (commands, menus, keybindings, views, settings, secrets, tools, host kinds).
- `protocol`: every JSON-RPC request and notification in both directions, as `Request` and `Notification` types with their method name and who answers them.
- `view`: the extension view model, made of building blocks, ready-made trees, tables, forms and streams, keyed patches and events.
- `limits`: what the app accepts from a program.

A slider, from API 1.1, is a node of type `slider`; with `advancing` the app moves its value by
itself at `rate` units per second until `max`, and it sends `slider_change` while dragging and
`slider_commit` on release:

```json
{ "key": "timeline", "type": "slider", "action": "seek", "value": 30, "max": 245,
  "format": "duration", "labels": "elapsed_remaining", "advancing": { "rate": 1 } }
```

`api_schema()` produces the JSON Schema the app publishes in `sdk/schema/`, for writing extensions in other languages.

To write an extension in Rust, use [`krad-extension-sdk`](https://crates.io/crates/krad-extension-sdk). For TypeScript or JavaScript, use [`@krad-ade/extension`](https://www.npmjs.com/package/@krad-ade/extension).

Licensed under MIT or Apache-2.0, at your option.
