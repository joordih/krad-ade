import type { Readable, Writable } from "node:stream";

import { encodeFrame, FrameDecoder } from "./framing.js";

export const METHOD_NOT_FOUND = -32601;
export const INVALID_PARAMS = -32602;
export const INTERNAL_ERROR = -32603;
export const REQUEST_FAILED = -32803;

const JSONRPC = "2.0";

export interface ResponseError {
  code: number;
  message: string;
}

export class RpcError extends Error {
  constructor(readonly code: number, message: string) {
    super(message);
  }
}

type Id = number | string;

interface Message {
  jsonrpc?: string;
  id?: Id;
  method?: string;
  params?: unknown;
  result?: unknown;
  error?: ResponseError;
}

export type RequestHandler = (method: string, params: unknown) => Promise<unknown> | unknown;
export type NotificationHandler = (method: string, params: unknown) => void;

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}

export class Connection {
  private nextId = 1;
  private readonly pending = new Map<Id, Pending>();
  private readonly decoder = new FrameDecoder();
  private onRequest: RequestHandler = (method) => {
    throw new RpcError(METHOD_NOT_FOUND, method);
  };
  private onNotification: NotificationHandler = () => {};
  private onClose: () => void = () => {};

  constructor(input: Readable, private readonly output: Writable) {
    input.on("data", (chunk: Buffer) => this.receive(chunk));
    input.on("end", () => this.close());
  }

  handleRequests(handler: RequestHandler): void {
    this.onRequest = handler;
  }

  handleNotifications(handler: NotificationHandler): void {
    this.onNotification = handler;
  }

  handleClose(handler: () => void): void {
    this.onClose = handler;
  }

  request(method: string, params: unknown): Promise<unknown> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.write({ jsonrpc: JSONRPC, id, method, params });
    });
  }

  notify(method: string, params: unknown): void {
    this.write({ jsonrpc: JSONRPC, method, params });
  }

  private write(message: Message): void {
    this.output.write(encodeFrame(message));
  }

  private receive(chunk: Buffer): void {
    for (const message of this.decoder.push(chunk) as Message[]) {
      this.dispatch(message);
    }
  }

  private dispatch(message: Message): void {
    if (message.method !== undefined && message.id !== undefined) {
      void this.answer(message.id, message.method, message.params);
      return;
    }
    if (message.method !== undefined) {
      this.onNotification(message.method, message.params);
      return;
    }
    if (message.id === undefined) {
      return;
    }
    const pending = this.pending.get(message.id);
    if (pending === undefined) {
      return;
    }
    this.pending.delete(message.id);
    if (message.error !== undefined) {
      pending.reject(new RpcError(message.error.code, message.error.message));
    } else {
      pending.resolve(message.result ?? null);
    }
  }

  private async answer(id: Id, method: string, params: unknown): Promise<void> {
    try {
      const result = await this.onRequest(method, params);
      this.write({ jsonrpc: JSONRPC, id, result: result ?? {} });
    } catch (error) {
      const code = error instanceof RpcError ? error.code : REQUEST_FAILED;
      const text = error instanceof Error ? error.message : String(error);
      this.write({ jsonrpc: JSONRPC, id, error: { code, message: text } });
    }
  }

  private close(): void {
    for (const pending of this.pending.values()) {
      pending.reject(new Error("the app closed the connection"));
    }
    this.pending.clear();
    this.onClose();
  }
}
