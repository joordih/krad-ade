import { PassThrough } from "node:stream";

import { encodeFrame, FrameDecoder } from "../dist/index.js";

export function fakeApp() {
  const toProgram = new PassThrough();
  const fromProgram = new PassThrough();
  const decoder = new FrameDecoder();
  const received = [];
  const waiters = [];
  fromProgram.on("data", (chunk) => {
    for (const message of decoder.push(chunk)) {
      const waiter = waiters.shift();
      if (waiter) waiter(message);
      else received.push(message);
    }
  });
  return {
    io: { input: toProgram, output: fromProgram, exit: () => {} },
    send(message) {
      toProgram.write(encodeFrame({ jsonrpc: "2.0", ...message }));
    },
    next() {
      const ready = received.shift();
      if (ready) return Promise.resolve(ready);
      return new Promise((resolve) => waiters.push(resolve));
    },
  };
}

export const INIT = {
  app: { name: "Krad ADE", version: "0.3.0" },
  api: "1.0",
  placement: "app",
  extension_folder: "/extensions/test",
  features: ["views", "commands"],
};
