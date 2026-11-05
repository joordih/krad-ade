import assert from "node:assert/strict";
import { test } from "node:test";

import { encodeFrame, FrameDecoder, FrameError, MAX_FRAME_BYTES } from "../dist/index.js";

test("a frame read back in pieces gives the same messages", () => {
  const messages = [{ jsonrpc: "2.0", id: 1, method: "shutdown", params: {} }, { text: "héllo ✓" }];
  const bytes = Buffer.concat(messages.map(encodeFrame));
  const decoder = new FrameDecoder();
  const read = [];
  for (let at = 0; at < bytes.length; at += 7) {
    read.push(...decoder.push(bytes.subarray(at, at + 7)));
  }
  assert.deepEqual(read, messages);
});

test("the length counts bytes, not characters", () => {
  const frame = encodeFrame({ text: "✓" }).toString("utf8");
  assert.match(frame, /^Content-Length: 14\r\n\r\n/);
});

test("frames without a length or too large are refused", () => {
  assert.throws(() => new FrameDecoder().push(Buffer.from("X-Other: 1\r\n\r\n{}")), FrameError);
  const huge = Buffer.from(`Content-Length: ${MAX_FRAME_BYTES + 1}\r\n\r\n`);
  assert.throws(() => new FrameDecoder().push(huge), FrameError);
});
