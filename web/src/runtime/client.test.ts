import { describe, expect, it } from "vitest";
import type { Message } from "../protocol/Message";
import { RuntimeClient, type SocketLike } from "./client";
import { decodeFrame, encodeMessage, TERMINAL_FRAME } from "./frames";

class FakeSocket implements SocketLike {
  binaryType = "blob";
  sent: Message[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: ArrayBuffer | Uint8Array }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  send(data: Uint8Array): void {
    const frame = decodeFrame(data);
    if (frame?.kind === "message") this.sent.push(frame.message);
  }

  close(): void {
    this.onclose?.();
  }

  deliver(message: Message): void {
    this.onmessage?.({ data: encodeMessage(message) });
  }

  welcome(run: number, firstRequest = 1 << 32): void {
    this.deliver({ type: "welcome", protocol: 1, build: "test", run, permissions: ["read", "drive"], first_request: firstRequest });
  }
}

function harness() {
  const sockets: FakeSocket[] = [];
  const scheduled: Array<() => void> = [];
  const client = new RuntimeClient({
    url: "ws://test/runtime",
    build: "web-test",
    socket: () => {
      const socket = new FakeSocket();
      sockets.push(socket);
      return socket;
    },
    schedule: (run) => scheduled.push(run),
  });
  return { client, sockets, scheduled };
}

describe("frames", () => {
  it("round-trips a message and reads a terminal frame", () => {
    const hello: Message = { type: "hello", protocol: 1, build: "x" };
    const frame = decodeFrame(encodeMessage(hello));
    expect(frame).toEqual({ kind: "message", message: hello });
    const terminal = new Uint8Array([TERMINAL_FRAME, 0, 0, 0, 0, 0, 0, 0, 9, 104, 105]);
    expect(decodeFrame(terminal)).toEqual({
      kind: "terminal",
      terminal: { terminal: 9, bytes: new Uint8Array([104, 105]) },
    });
  });
});

describe("RuntimeClient", () => {
  it("greets, subscribes from scratch and resumes from the last seq after a reconnect", () => {
    const { client, sockets, scheduled } = harness();
    client.connect();
    const first = sockets[0]!;
    first.onopen?.();
    expect(first.sent[0]).toMatchObject({ type: "hello", protocol: 1 });
    first.welcome(7);
    expect(first.sent[1]).toEqual({ type: "subscribe" });
    first.deliver({ type: "feed", feed: "event", seq: 41, kind: { type: "pong", request: 1 } });
    first.close();
    expect(scheduled).toHaveLength(1);
    scheduled[0]!();
    const second = sockets[1]!;
    second.onopen?.();
    second.welcome(7);
    expect(second.sent[1]).toEqual({ type: "subscribe", since: 41 });
  });

  it("starts from a snapshot again when the runtime restarted", () => {
    const { client, sockets, scheduled } = harness();
    client.connect();
    sockets[0]!.onopen?.();
    sockets[0]!.welcome(7);
    sockets[0]!.deliver({ type: "feed", feed: "event", seq: 3, kind: { type: "pong", request: 1 } });
    sockets[0]!.close();
    scheduled[0]!();
    sockets[1]!.onopen?.();
    sockets[1]!.welcome(8);
    expect(sockets[1]!.sent[1]).toEqual({ type: "subscribe" });
  });

  it("numbers commands from the welcome and resolves an answer that arrives as an event", async () => {
    const { client, sockets } = harness();
    client.connect();
    const socket = sockets[0]!;
    socket.onopen?.();
    socket.welcome(1, 500);
    const asked = client.ask({ area: "editor", verb: "save", buffer: 1 });
    const sent = socket.sent.at(-1);
    expect(sent).toMatchObject({ type: "command", id: 500 });
    socket.deliver({ type: "reply", id: 500, reply: { outcome: "accepted" } });
    socket.deliver({ type: "feed", feed: "event", seq: 2, kind: { type: "answered", request: 500, answer: { kind: "done" } } });
    await expect(asked).resolves.toEqual({ kind: "done" });
  });

  it("rejects a refused command with its code", async () => {
    const { client, sockets } = harness();
    client.connect();
    sockets[0]!.onopen?.();
    sockets[0]!.welcome(1, 10);
    const asked = client.ask({ area: "editor", verb: "close", buffer: 9 });
    sockets[0]!.deliver({ type: "reply", id: 10, reply: { outcome: "refused", code: "not_found", message: "no buffer" } });
    await expect(asked).rejects.toMatchObject({ code: "not_found" });
  });

  it("stops retrying when the runtime refuses the hello", () => {
    const { client, sockets, scheduled } = harness();
    const statuses: string[] = [];
    client.onStatus((status) => statuses.push(status.status));
    client.connect();
    sockets[0]!.onopen?.();
    sockets[0]!.deliver({ type: "refused", code: "forbidden", message: "not paired" });
    sockets[0]!.close();
    expect(statuses).toContain("refused");
    expect(scheduled).toHaveLength(0);
  });
});
