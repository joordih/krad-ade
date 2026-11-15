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
}

type Waiting =
  | { kind: "reply"; resolve: (reply: Reply) => void; reject: (error: Error) => void }
  | { kind: "snapshot"; resolve: (snapshot: Snapshot) => void; reject: (error: Error) => void }
  | { kind: "paired"; resolve: (answer: PairingAnswer) => void; reject: (error: Error) => void };

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
    this.open();
  }

  close(): void {
    this.stopped = true;
    this.socket?.close();
    this.fail(new Error("the client closed"));
    this.tell({ status: "closed" });
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
    const id = this.request();
    return new Promise((resolve, reject) => {
      this.waiting.set(id, { kind: "reply", resolve, reject });
      if (!this.write({ type: "command", id, command })) {
        this.waiting.delete(id);
        reject(new Error("the runtime is not connected"));
      }
    });
  }

  async ask(command: Command): Promise<Answer> {
    const id = this.request();
    const answered = new Promise<Answer>((resolve) => this.answers.set(id, resolve));
    const reply = await new Promise<Reply>((resolve, reject) => {
      this.waiting.set(id, { kind: "reply", resolve, reject });
      if (!this.write({ type: "command", id, command })) {
        this.waiting.delete(id);
        reject(new Error("the runtime is not connected"));
      }
    }).catch((error: unknown) => {
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
    const id = this.request();
    return new Promise((resolve, reject) => {
      this.waiting.set(id, { kind: "snapshot", resolve, reject });
      if (!this.write({ type: "ask_snapshot", id, scope: "runtime" })) {
        this.waiting.delete(id);
        reject(new Error("the runtime is not connected"));
      }
    });
  }

  pairing(command: PairingCommand): Promise<PairingAnswer> {
    const id = this.request();
    return new Promise((resolve, reject) => {
      this.waiting.set(id, { kind: "paired", resolve, reject });
      if (!this.write({ type: "pairing", id, command })) {
        this.waiting.delete(id);
        reject(new Error("the runtime is not connected"));
      }
    });
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

  private write(message: Message): boolean {
    if (!this.socket || !this.welcome) {
      return message.type === "hello" && this.socket !== undefined && this.sendRaw(message);
    }
    return this.sendRaw(message);
  }

  private sendRaw(message: Message): boolean {
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
    this.fail(new Error("the runtime connection closed"));
    if (this.stopped) return;
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
    if (this.takesDelegations) this.sendRaw({ type: "take_delegations" });
    this.tell({ status: "open", welcome });
  }

  private take(message: Message): void {
    switch (message.type) {
      case "welcome":
        return this.welcomed(message);
      case "refused":
        if (!this.welcome) {
          this.stopped = true;
          this.tell({ status: "refused", code: message.code, message: message.message });
        }
        return;
      case "feed":
        return this.fed(message);
      case "reply": {
        const waiting = this.waiting.get(message.id);
        if (waiting?.kind === "reply") {
          this.waiting.delete(message.id);
          waiting.resolve(message.reply);
        } else if (waiting) {
          this.waiting.delete(message.id);
          const reply = message.reply;
          waiting.reject(
            reply.outcome === "refused"
              ? new RuntimeError(reply.code, reply.message)
              : new RuntimeError("unknown", "the runtime answered with a plain reply"),
          );
        }
        return;
      }
      case "snapshot": {
        const waiting = this.waiting.get(message.id);
        if (waiting?.kind === "snapshot") {
          this.waiting.delete(message.id);
          waiting.resolve(message.snapshot);
        }
        return;
      }
      case "paired": {
        const waiting = this.waiting.get(message.id);
        if (waiting?.kind === "paired") {
          this.waiting.delete(message.id);
          waiting.resolve(message.answer);
        }
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
