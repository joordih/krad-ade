const HEADER_SEPARATOR = "\r\n\r\n";
const CONTENT_LENGTH = "content-length:";
export const MAX_FRAME_BYTES = 64 * 1024 * 1024;

export function encodeFrame(message: unknown): Buffer {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  return Buffer.concat([Buffer.from(`Content-Length: ${body.length}${HEADER_SEPARATOR}`, "ascii"), body]);
}

export class FrameError extends Error {}

export class FrameDecoder {
  private buffer: Buffer = Buffer.alloc(0);

  push(chunk: Buffer): unknown[] {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    const messages: unknown[] = [];
    for (;;) {
      const headerEnd = this.buffer.indexOf(HEADER_SEPARATOR);
      if (headerEnd < 0) {
        return messages;
      }
      const length = contentLength(this.buffer.subarray(0, headerEnd).toString("ascii"));
      const start = headerEnd + HEADER_SEPARATOR.length;
      if (this.buffer.length < start + length) {
        return messages;
      }
      const body = this.buffer.subarray(start, start + length).toString("utf8");
      this.buffer = this.buffer.subarray(start + length);
      messages.push(JSON.parse(body));
    }
  }
}

function contentLength(header: string): number {
  const line = header
    .split("\r\n")
    .find((candidate) => candidate.toLowerCase().startsWith(CONTENT_LENGTH));
  if (line === undefined) {
    throw new FrameError("a frame arrived without its Content-Length");
  }
  const length = Number.parseInt(line.slice(CONTENT_LENGTH.length).trim(), 10);
  if (!Number.isSafeInteger(length) || length < 0 || length > MAX_FRAME_BYTES) {
    throw new FrameError(`a frame announced an invalid length: ${line}`);
  }
  return length;
}
