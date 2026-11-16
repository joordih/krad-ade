import type { Message } from "../protocol/Message";

export const MESSAGE_FRAME = 1;
export const TERMINAL_FRAME = 2;
export const RUNNER_TERMINAL_FRAME = 3;
const TERMINAL_ID_BYTES = 8;

export type TerminalBytes = { terminal: number; bytes: Uint8Array };

export type Frame =
  | { kind: "message"; message: Message }
  | { kind: "terminal"; terminal: TerminalBytes }
  | { kind: "runner_terminal"; runner: number; terminal: TerminalBytes };

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function encodeMessage(message: Message): Uint8Array {
  const json = encoder.encode(JSON.stringify(message));
  const frame = new Uint8Array(json.length + 1);
  frame[0] = MESSAGE_FRAME;
  frame.set(json, 1);
  return frame;
}

export function decodeFrame(data: ArrayBuffer | Uint8Array): Frame | undefined {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
  const kind = bytes[0];
  if (kind === MESSAGE_FRAME) {
    const message = JSON.parse(decoder.decode(bytes.subarray(1))) as Message;
    return { kind: "message", message };
  }
  if (kind === TERMINAL_FRAME && bytes.length >= 1 + TERMINAL_ID_BYTES) {
    const view = new DataView(bytes.buffer, bytes.byteOffset + 1, TERMINAL_ID_BYTES);
    const terminal = Number(view.getBigUint64(0, false));
    return {
      kind: "terminal",
      terminal: { terminal, bytes: bytes.slice(1 + TERMINAL_ID_BYTES) },
    };
  }
  if (kind === RUNNER_TERMINAL_FRAME && bytes.length >= 1 + 2 * TERMINAL_ID_BYTES) {
    const view = new DataView(bytes.buffer, bytes.byteOffset + 1, 2 * TERMINAL_ID_BYTES);
    const runner = Number(view.getBigUint64(0, false));
    const terminal = Number(view.getBigUint64(TERMINAL_ID_BYTES, false));
    return {
      kind: "runner_terminal",
      runner,
      terminal: { terminal, bytes: bytes.slice(1 + 2 * TERMINAL_ID_BYTES) },
    };
  }
  return undefined;
}
