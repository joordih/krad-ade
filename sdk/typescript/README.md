# @krad-ade/extension

Write the program of a Krad ADE extension in TypeScript or JavaScript. The app starts the program
with Node, talks to it over stdin and stdout with JSON-RPC 2.0 framed like LSP, and draws the views
it describes with its own components and theme. The protocol is described by
`sdk/schema/krad-extension-api.json`.

## Getting started

An extension is a folder with an `extension.toml` and the program it names:

```toml
id = "hello"
name = "Hello"
version = "0.1.0"
api = "1.0"

[program]
placement = "app"
runtime = "node"
args = ["dist/main.js"]

[[contributes.command]]
id = "greet"
title = "Hello: Greet"

[[contributes.view]]
id = "greetings"
title = "Greetings"
zone = "sidebar"
icon = "magic"
```

```ts
import { createExtension, ui } from "@krad-ade/extension";

let count = 0;

const extension = createExtension({
  commands: {
    greet: (_target, { app }) => app.toast("Hello from an extension", "success"),
  },
  views: {
    greetings: {
      render: () =>
        ui.column("root", [
          ui.text("count", `Clicked ${count} times`),
          ui.button("more", "more", { label: "Click", variant: "primary" }),
        ]),
      onEvent: (event, view, { app }) => {
        if (event.kind === "click" && event.action === "more") {
          count += 1;
          app.patchView(view, [{ op: "replace", node: ui.text("count", `Clicked ${count} times`) }]);
        }
      },
    },
  },
});
```

Add the folder from Settings → Languages → Extensions → Add folder, and trust it when asked.

## Sliders

A slider needs `api = "1.1"`. While `advancing` is set the app moves the value by itself, `rate`
units per second from when the node arrived, and stops at `max`, so a music timeline needs no patch
every second. Dragging sends `slider_change` (throttled) and releasing sends `slider_commit`; the
slider then waits at the committed value until the program sends the node again.

```ts
import { createExtension, isSliderEvent, patch, ui } from "@krad-ade/extension";

const timeline = (position: number, playing: boolean) =>
  ui.slider("timeline", "seek", {
    value: position,
    max: 245,
    format: "duration",
    labels: "elapsed_remaining",
    advancing: playing ? { rate: 1 } : null,
  });

createExtension({
  views: {
    player: {
      render: () =>
        ui.column("root", [
          timeline(0, true),
          ui.slider("volume", "volume", { value: 0.8, max: 1, step: 0.05, format: "percent", labels: "value" }),
        ]),
      onEvent: (event, view, { app }) => {
        if (isSliderEvent(event) && event.kind === "slider_commit" && event.action === "seek") {
          app.patchView(view, [patch.replace(timeline(event.value, true))]);
        }
      },
    },
  },
});
```

## Rules

- stdout belongs to the protocol. Log with `app.log(level, message)` or write to stderr; Settings
  shows the program's stderr.
- Ids (commands, views, settings, secrets, tools) are lowercase letters, digits, `-`, `_` and `.`.
  Node keys and action ids are any non-empty text, unique within a view.
- Views describe, never run code: no HTML, no scripts. Colors are theme tokens.
- Lists and tables are virtualized: give `count` and answer `rows` for the range the app asks for.

## Develop

```sh
npm install
npm test
```

`npm test` builds, runs the tests and checks that the SDK names every method of the schema.
