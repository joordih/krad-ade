import { describe, expect, it } from "vitest";
import { fileAnswer, formatBytes, unreadableText } from "./fileRead";

describe("the read answer", () => {
  it("maps text, truncation and unreadable reasons", () => {
    expect(fileAnswer({ kind: "file_text", text: "hi", len: 2, truncated: false })).toEqual({
      kind: "text",
      text: "hi",
      len: 2,
      truncated: false,
    });
    expect(fileAnswer({ kind: "unreadable", unreadable: { reason: "too_large", len: 12_582_912, limit: 8_388_608 } })).toEqual({
      kind: "error",
      message: "This file is too large to show (12 MB; the limit is 8 MB).",
    });
    expect(fileAnswer({ kind: "unreadable", unreadable: { reason: "not_text" } })).toEqual({
      kind: "error",
      message: "This file is not text, so it cannot be shown here.",
    });
    expect(fileAnswer({ kind: "done" })).toBeUndefined();
  });

  it("says every unreadable reason in words", () => {
    expect(unreadableText({ reason: "missing" })).toBe("This file no longer exists.");
    expect(unreadableText({ reason: "outside" })).toBe("This file is outside the group's folder.");
  });

  it("formats byte counts", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2 KB");
    expect(formatBytes(8 * 1024 * 1024)).toBe("8 MB");
    expect(formatBytes(1.4 * 1024 * 1024)).toBe("1.4 MB");
  });
});
