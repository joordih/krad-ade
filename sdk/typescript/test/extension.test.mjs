import assert from "node:assert/strict";
import { test } from "node:test";

import { createExtension, METHOD_NOT_FOUND, ui } from "../dist/index.js";
import { fakeApp, INIT } from "./helpers.mjs";

test("initialize answers the features the extension offers", async () => {
  const app = fakeApp();
  createExtension({ commands: {}, views: {} }, app.io);
  app.send({ id: 1, method: "initialize", params: INIT });
  const reply = await app.next();
  assert.equal(reply.id, 1);
  assert.ok(reply.result.features.includes("commands"));
  assert.ok(reply.result.features.includes("views"));
  assert.ok(!reply.result.features.includes("tools"));
});

test("a command request reaches its handler with its target", async () => {
  const app = fakeApp();
  const seen = [];
  createExtension({ commands: { refresh: (target) => seen.push(target) } }, app.io);
  app.send({ id: 1, method: "initialize", params: INIT });
  await app.next();
  const target = { kind: "file", path: "/repo/Dockerfile" };
  app.send({ id: 2, method: "command/execute", params: { command: "refresh", target } });
  const reply = await app.next();
  assert.deepEqual(reply, { jsonrpc: "2.0", id: 2, result: {} });
  assert.deepEqual(seen, [target]);
});

test("unknown methods and commands are refused with an error reply", async () => {
  const app = fakeApp();
  createExtension({ commands: {} }, app.io);
  app.send({ id: 1, method: "initialize", params: INIT });
  await app.next();
  app.send({ id: 2, method: "view/teleport", params: {} });
  assert.equal((await app.next()).error.code, METHOD_NOT_FOUND);
  app.send({ id: 3, method: "command/execute", params: { command: "missing" } });
  assert.equal((await app.next()).error.code, METHOD_NOT_FOUND);
});

test("a view renders its tree and receives its events", async () => {
  const app = fakeApp();
  const events = [];
  createExtension(
    {
      views: {
        containers: {
          render: () => ui.column("root", [ui.button("refresh", "refresh", { label: "Refresh" })]),
          onEvent: (event) => events.push(event),
        },
      },
    },
    app.io,
  );
  app.send({ id: 1, method: "initialize", params: INIT });
  await app.next();
  app.send({ id: 2, method: "view/render", params: { view: "containers" } });
  const reply = await app.next();
  assert.equal(reply.result.type, "flex");
  assert.equal(reply.result.children[0].type, "button");
  app.send({ method: "view/event", params: { view: "containers", event: { kind: "click", action: "refresh" } } });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(events, [{ kind: "click", action: "refresh" }]);
});

test("calls to the app resolve with its answer", async () => {
  const app = fakeApp();
  const extension = createExtension({}, app.io);
  const answer = extension.app.confirm({ title: "Remove web?", confirm: "Remove", tone: "error" });
  const request = await app.next();
  assert.equal(request.method, "window/confirm");
  app.send({ id: request.id, result: "confirmed" });
  assert.equal(await answer, "confirmed");
});

test("a tool answering text is wrapped as tool content", async () => {
  const app = fakeApp();
  createExtension({ tools: { list_containers: () => "web\ndb" } }, app.io);
  app.send({ id: 1, method: "initialize", params: INIT });
  await app.next();
  app.send({ id: 2, method: "tool/call", params: { tool: "list_containers", arguments: {} } });
  assert.deepEqual((await app.next()).result, { content: [{ type: "text", text: "web\ndb" }], outcome: "done" });
});
