import { describe, expect, it } from "vitest";
import { effortLabel, effortTrigger, levelShare, meterStep, permissionLabel, sendState, windowLabel } from "./controls";

describe("composer controls", () => {
  it("labels permission modes and efforts like the desktop", () => {
    expect(permissionLabel(undefined)).toBe("Ask before edits");
    expect(permissionLabel("acceptEdits")).toBe("Accept edits");
    expect(effortLabel(null)).toBe("Default");
    expect(effortLabel("xhigh")).toBe("Extra high");
  });

  it("hides, greys or opens the effort control by how many levels the model has", () => {
    expect(effortTrigger(false, 5)).toBe("hidden");
    expect(effortTrigger(true, 1)).toBe("disabled");
    expect(effortTrigger(true, 0)).toBe("disabled");
    expect(effortTrigger(true, 3)).toBe("enabled");
  });

  it("lights the meter up to the chosen level, else the model's default", () => {
    expect(meterStep(["low", "medium", "high"], "medium", null)).toBe(1);
    expect(meterStep(["low", "medium", "high"], null, "high")).toBe(2);
    expect(meterStep(["low", "medium"], null, null)).toBeUndefined();
    expect(levelShare(1, 3)).toBe(0.5);
    expect(levelShare(0, 1)).toBe(1);
  });

  it("turns the send button into stop only while the agent works and nothing is typed", () => {
    expect(sendState(true, true)).toBe("stop");
    expect(sendState(true, false)).toBe("idle");
    expect(sendState(false, true)).toBe("ready");
  });

  it("writes a context window like 200K or 1M", () => {
    expect(windowLabel(200_000)).toBe("200K");
    expect(windowLabel(1_000_000)).toBe("1M");
    expect(windowLabel(900)).toBe("900");
  });
});
