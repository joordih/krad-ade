# krad-extension-api

The protocol between [Krad ADE](https://github.com/joordih/krad-ade) and extension programs. It has no I/O; it only holds types:

- `manifest`: the `extension.toml` an extension ships: its program, permissions, activation and contributions (commands, menus, keybindings, views, titlebar items, settings, secrets, tools, host kinds).
- `protocol`: every JSON-RPC request and notification in both directions, as `Request` and `Notification` types with their method name and who answers them.
- `view`: the extension view model, made of building blocks, ready-made trees, tables, forms and streams, keyed patches and events.
- `limits`: what the app accepts from a program.

Since API 1.1 an extension can put buttons in the window titlebar, beside the ADE/Editor switch.
A button runs a command or opens a view of zone `popover` anchored under it, and the program changes
its icon, tooltip, highlight or visibility with the `titlebar/set` notification
(`SetTitlebarItemParams`):

```toml
api = "1.1"

[[contributes.view]]
id = "now-playing"
title = "Now playing"
zone = "popover"

[[contributes.titlebar_item]]
id = "now-playing"
title = "Now playing"
icon = "play"
side = "left"
view = "now-playing"
```

`api_schema()` produces the JSON Schema the app publishes in `sdk/schema/`, for writing extensions in other languages.

To write an extension in Rust, use [`krad-extension-sdk`](https://crates.io/crates/krad-extension-sdk). For TypeScript or JavaScript, use [`@krad-ade/extension`](https://www.npmjs.com/package/@krad-ade/extension).

Licensed under MIT or Apache-2.0, at your option.
