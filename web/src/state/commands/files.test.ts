import { describe, expect, it } from "vitest";
import {
  listFolderCommand,
  MAX_READ_BYTES,
  readFileCommand,
  unwatchFolderCommand,
  watchFolderCommand,
} from "./files";

describe("file commands", () => {
  it("names the environment and the ask verb on every request", () => {
    expect(listFolderCommand("main", "/r")).toEqual({
      area: "file",
      environment: "main",
      ask: { verb: "list", dir: "/r" },
    });
    expect(watchFolderCommand("main", "/r")).toEqual({
      area: "file",
      environment: "main",
      ask: { verb: "watch", dir: "/r" },
    });
    expect(unwatchFolderCommand("main", "/r")).toEqual({
      area: "file",
      environment: "main",
      ask: { verb: "unwatch", dir: "/r" },
    });
  });

  it("reads from the start, capped by the runtime's byte limit and truncating", () => {
    expect(readFileCommand("main", "/r/src/main.rs")).toEqual({
      area: "file",
      environment: "main",
      ask: {
        verb: "read",
        path: "/r/src/main.rs",
        from: 0,
        limit: MAX_READ_BYTES,
        over: "truncate",
      },
    });
  });
});
