import { describe, expect, it, vi } from "vitest";
import { hexOf, newResumeId } from "./ids";

describe("resume ids", () => {
  it("writes bytes as two hex digits each", () => {
    expect(hexOf(new Uint8Array([0, 15, 255]))).toBe("000fff");
  });

  it("builds a v4 UUID when the browser has no randomUUID", () => {
    vi.stubGlobal("crypto", { getRandomValues: (bytes: Uint8Array) => bytes.fill(0xff) });
    expect(newResumeId()).toBe("ffffffff-ffff-4fff-bfff-ffffffffffff");
    vi.unstubAllGlobals();
  });
});
