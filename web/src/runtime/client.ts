import type { AgentRef } from "../protocol/AgentRef";
import type { Answer } from "../protocol/Answer";
import type { Command } from "../protocol/Command";
import type { DelegatedRequest } from "../protocol/DelegatedRequest";
import type { Feed } from "../protocol/Feed";
import type { Message } from "../protocol/Message";
import type { PairingAnswer } from "../protocol/PairingAnswer";
import type { PairingCommand } from "../protocol/PairingCommand";
import type { Reply } from "../protocol/Reply";
import type { Response } from "../protocol/Response";
import type { Snapshot } from "../protocol/Snapshot";
import type { Welcome } from "../protocol/Welcome";
import { decodeFrame, encodeMessage, type TerminalBytes } from "./frames";

export const PROTOCOL_VERSION = 1;
export const RUNTIME_PATH = "/runtime";
const FIRST_BACKOFF_MS = 1_000;
const MAX_BACKOFF_MS = 30_000;
const NOT_CONNECTED = "the runtime is not connected";
const CLIENT_CLOSED = "the client closed";
const CONNECTION_CLOSED = "the runtime connection closed";
const PLAIN_REPLY = "the runtime answered with a plain reply";
const UNKNOWN_CODE = "unknown";

export interface SocketLike {
  binaryType: string;
  send(data: Uint8Array): void;
  close(): void;
  onopen: (() => void) | null;
  onmessage: ((event: { data: ArrayBuffer | Uint8Array }) => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
}

export type ConnectionStatus =
  | { status: "connecting" }
  | { status: "open"; welcome: Welcome }
  | { status: "waiting"; retryInMs: number }
  | { status: "refused"; code: string; message: string }
  | { status: "closed" };

export interface ClientOptions {
  url?: string;
  build: string;
  socket?: (url: string) => SocketLike;
  schedule?: (run: () => void, afterMs: number) => unknown;
  parent?: { client: RuntimeClient; runner: number };
}

type Settle<T> = { resolve: (value: T) => void; reject: (error: Error) => void };

type Waiting =
  | ({ kind: "reply" } & Settle<Reply>)
  | ({ kind: "snapshot" } & Settle<Snapshot>)
  | ({ kind: "paired" } & Settle<PairingAnswer>);

type Listener<T> = (value: T) => void;

export class RuntimeError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

function defaultUrl(): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${location.host}${RUNTIME_PATH}`;
}

function defaultSocket(url: string): SocketLike {
  return new WebSocket(url) as unknown as SocketLike;
}

export class RuntimeClient {
  private socket: SocketLike | undefined;
  private welcome: Welcome | undefined;
  private nextRequest = 1;
  private lastSeq: number | undefined;
  private lastRun: number | undefined;
  private backoff = FIRST_BACKOFF_MS;
  private stopped = false;
  private readonly waiting = new Map<number, Waiting>();
  private readonly answers = new Map<number, Listener<Answer>>();
  private readonly terminals = new Map<number, Listener<Uint8Array>>();
  private readonly feeds = new Set<Listener<Feed>>();
  private readonly statuses = new Set<Listener<ConnectionStatus>>();
  private readonly delegates = new Set<Listener<DelegatedRequest>>();
  private takesDelegations = false;
  private readonly children = new Map<number, RuntimeClient>();
  private readonly url: string;
  private readonly makeSocket: (url: string) => SocketLike;
  private readonly schedule: (run: () => void, afterMs: number) => unknown;

  constructor(private readonly options: ClientOptions) {
    this.url = options.url ?? defaultUrl();
    this.makeSocket = options.socket ?? defaultSocket;
    this.schedule = options.schedule ?? ((run, afterMs) => setTimeout(run, afterMs));
  }

  connect(): void {
    this.stopped = false;
    if (this.options.parent) {
      this.options.parent.client.children.set(this.options.parent.runner, this);
      if (this.options.parent.client.welcome) this.open();
      return;
    }
    this.open();
  }

  close(): void {
    this.stopped = true;
    if (this.options.parent) {
      this.options.parent.client.children.delete(this.options.parent.runner);
    } else {
      this.socket?.close();
    }
    this.fail(new Error(CLIENT_CLOSED));
    this.tell({ status: "closed" });
  }

  relay(runner: number): RuntimeClient {
    const known = this.children.get(runner);
    if (known) return known;
    return new RuntimeClient({ url: this.url, build: this.options.build, parent: { client: this, runner } });
  }

  get runner(): number | undefined {
    return this.options.parent?.runner;
  }

  get permissions(): Welcome["permissions"] {
    return this.welcome?.permissions;
  }

  onFeed(listener: Listener<Feed>): () => void {
    this.feeds.add(listener);
    return () => this.feeds.delete(listener);
  }

  onStatus(listener: Listener<ConnectionStatus>): () => void {
    this.statuses.add(listener);
    return () => this.statuses.delete(listener);
  }

  onDelegated(listener: Listener<DelegatedRequest>): () => void {
    this.delegates.add(listener);
    if (!this.takesDelegations) {
      this.takesDelegations = true;
      this.write({ type: "take_delegations" });
    }
    return () => this.delegates.delete(listener);
  }

  respond(delegation: number, response: Response): Promise<Reply> {
    return this.send({ area: "bus", verb: "respond", delegation, response });
  }

  send(command: Command): Promise<Reply> {
    return this.exchange(this.request(), (id) => ({ type: "command", id, command }), (settle) => ({ kind: "reply", ...settle }));
  }

  async ask(command: Command): Promise<Answer> {
    const id = this.request();
    const answered = new Promise<Answer>((resolve) => this.answers.set(id, resolve));
    const reply = await this.exchange<Reply>(
      id,
      () => ({ type: "command", id, command }),
      (settle) => ({ kind: "reply", ...settle }),
    ).catch((error: unknown) => {
      this.answers.delete(id);
      throw error;
    });
    if (reply.outcome === "refused") {
      this.answers.delete(id);
      throw new RuntimeError(reply.code, reply.message);
    }
    return answered;
  }

  snapshot(): Promise<Snapshot> {
    return this.exchange(
      this.request(),
      (id) => ({ type: "ask_snapshot", id, scope: "runtime" }),
      (settle) => ({ kind: "snapshot", ...settle }),
    );
  }

  pairing(command: PairingCommand): Promise<PairingAnswer> {
    return this.exchange(this.request(), (id) => ({ type: "pairing", id, command }), (settle) => ({ kind: "paired", ...settle }));
  }

  watchTerminal(terminal: AgentRef, listener: Listener<Uint8Array>): Promise<Reply> {
    this.terminals.set(terminal.terminal, listener);
    return this.send({ area: "terminal", verb: "watch", terminal });
  }

  unwatchTerminal(terminal: AgentRef): void {
    this.terminals.delete(terminal.terminal);
  }

  private request(): number {
    const id = this.nextRequest;
    this.nextRequest += 1;
    return id;
  }

  private exchange<T>(id: number, message: (id: number) => Message, waiting: (settle: Settle<T>) => Waiting): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      this.waiting.set(id, waiting({ resolve, reject }));
      if (!this.write(message(id))) {
        this.waiting.delete(id);
        reject(new Error(NOT_CONNECTED));
      }
    });
  }

  private settled(id: number): Waiting | undefined {
    const waiting = this.waiting.get(id);
    this.waiting.delete(id);
    return waiting;
  }

  private connected(): boolean {
    const parent = this.options.parent;
    return parent ? parent.client.welcome !== undefined : this.socket !== undefined;
  }

  private write(message: Message): boolean {
    if (!this.connected() || !this.welcome) {
      return message.type === "hello" && this.connected() && this.sendRaw(message);
    }
    return this.sendRaw(message);
  }

  private sendRaw(message: Message): boolean {
    const parent = this.options.parent;
    if (parent) {
      return parent.client.sendRaw({ type: "via_runner", runner: parent.runner, message });
    }
    try {
      this.socket?.send(encodeMessage(message));
      return this.socket !== undefined;
    } catch {
      return false;
    }
  }

  private tell(status: ConnectionStatus): void {
    for (const listener of this.statuses) listener(status);
  }

  private open(): void {
    this.tell({ status: "connecting" });
    if (this.options.parent) {
      this.sendRaw({ type: "hello", protocol: PROTOCOL_VERSION, build: this.options.build });
      return;
    }
    const socket = this.makeSocket(this.url);
    socket.binaryType = "arraybuffer";
    this.socket = socket;
    socket.onopen = () => {
      this.sendRaw({ type: "hello", protocol: PROTOCOL_VERSION, build: this.options.build });
    };
    socket.onmessage = (event) => this.heard(event.data);
    socket.onclose = () => this.lost();
    socket.onerror = () => socket.close();
  }

  private lost(): void {
    this.socket = undefined;
    this.welcome = undefined;
    this.fail(new Error(CONNECTION_CLOSED));
    for (const child of this.children.values()) child.lost();
    if (this.stopped || this.options.parent) return;
    const retryInMs = this.backoff;
    this.backoff = Math.min(this.backoff * 2, MAX_BACKOFF_MS);
    this.tell({ status: "waiting", retryInMs });
    this.schedule(() => {
      if (!this.stopped) this.open();
    }, retryInMs);
  }

  private fail(error: Error): void {
    for (const waiting of this.waiting.values()) waiting.reject(error);
    this.waiting.clear();
    this.answers.clear();
  }

  private heard(data: ArrayBuffer | Uint8Array): void {
    const frame = decodeFrame(data);
    if (!frame) return;
    if (frame.kind === "terminal") return this.heardTerminal(frame.terminal);
    if (frame.kind === "runner_terminal") {
      this.children.get(frame.runner)?.heardTerminal(frame.terminal);
      return;
    }
    this.take(frame.message);
  }

  private heardTerminal(bytes: TerminalBytes): void {
    this.terminals.get(bytes.terminal)?.(bytes.bytes);
  }

  private welcomed(welcome: Welcome): void {
    this.welcome = welcome;
    this.backoff = FIRST_BACKOFF_MS;
    this.nextRequest = welcome.first_request ?? this.nextRequest;
    const sameRun = this.lastRun === welcome.run && this.lastSeq !== undefined;
    this.lastRun = welcome.run;
    this.sendRaw(sameRun ? { type: "subscribe", since: this.lastSeq } : { type: "subscribe" });
    if (this.takesDelegations && !this.options.parent) this.sendRaw({ type: "take_delegations" });
    this.tell({ status: "open", welcome });
    for (const child of this.children.values()) {
      if (!child.stopped) child.open();
    }
  }

  private take(message: Message): void {
    switch (message.type) {
      case "welcome":
        return this.welcomed(message);
      case "refused":
        if (!this.welcome) {
          if (!this.options.parent) this.stopped = true;
          this.tell({ status: "refused", code: message.code, message: message.message });
        }
        return;
      case "via_runner":
        this.children.get(message.runner)?.take(message.message);
        return;
      case "feed":
        return this.fed(message);
      case "reply": {
        const waiting = this.settled(message.id);
        const reply = message.reply;
        if (waiting?.kind === "reply") waiting.resolve(reply);
        else if (waiting)
          waiting.reject(
            reply.outcome === "refused" ? new RuntimeError(reply.code, reply.message) : new RuntimeError(UNKNOWN_CODE, PLAIN_REPLY),
          );
        return;
      }
      case "snapshot": {
        const waiting = this.settled(message.id);
        if (waiting?.kind === "snapshot") waiting.resolve(message.snapshot);
        return;
      }
      case "paired": {
        const waiting = this.settled(message.id);
        if (waiting?.kind === "paired") waiting.resolve(message.answer);
        return;
      }
      case "delegated":
        for (const listener of this.delegates) listener(message);
        return;
      default:
        return;
    }
  }

  private fed(feed: Feed): void {
    this.lastSeq = feed.seq;
    if (feed.feed === "event" && feed.kind.type === "answered") {
      const listener = this.answers.get(feed.kind.request);
      if (listener) {
        this.answers.delete(feed.kind.request);
        listener(feed.kind.answer);
      }
    }
    for (const listener of this.feeds) listener(feed);
  }
}
