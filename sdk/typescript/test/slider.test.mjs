import assert from "node:assert/strict";
import { test } from "node:test";

import { createExtension, isSliderEvent, ui } from "../dist/index.js";
import { fakeApp, INIT } from "./helpers.mjs";

test("a slider builds the node the app reads", () => {
  const timeline = ui.slider("timeline", "seek", {
    value: 30,
    max: 245,
    format: "duration",
    labels: "elapsed_remaining",
    advancing: { rate: 1 },
  });
  assert.deepEqual(timeline, {
    key: "timeline",
    type: "slider",
    action: "seek",
    value: 30,
    max: 245,
    format: "duration",
    labels: "elapsed_remaining",
    advancing: { rate: 1 },
  });
  const volume = ui.slider("volume", "volume", { value: 0.5, max: 1, step: 0.05, format: "percent", labels: "value" });
  assert.equal(volume.step, 0.05);
  assert.equal(volume.advancing, undefined);
});

test("slider changes and commits reach the view as typed events", async () => {
  const app = fakeApp();
  const values = [];
  createExtension(
    {
      views: {
        player: {
          render: () => ui.column("root", [ui.slider("timeline", "seek", { value: 0, max: 245 })]),
          onEvent: (event) => {
            if (isSliderEvent(event)) {
              values.push([event.kind, event.action, event.value]);
            }
          },
        },
      },
    },
    app.io,
  );
  app.send({ id: 1, method: "initialize", params: INIT });
  await app.next();
  app.send({ method: "view/event", params: { view: "player", event: { kind: "slider_change", action: "seek", value: 40 } } });
  app.send({ method: "view/event", params: { view: "player", event: { kind: "slider_commit", action: "seek", value: 42 } } });
  app.send({ method: "view/event", params: { view: "player", event: { kind: "click", action: "play" } } });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(values, [
    ["slider_change", "seek", 40],
    ["slider_commit", "seek", 42],
  ]);
});
