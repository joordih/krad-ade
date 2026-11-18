import { describe, expect, it } from "vitest";
import { badgeLook, stateDot, tabMarks } from "./tabLook";

describe("a tab's look", () => {
  it("fills the badge with the harness colour, leaves shells neutral and files bare", () => {
    expect(badgeLook({ kind: "Chat", harness: "ClaudeCode" })).toEqual({ icon: "brand-claude", fill: "brand" });
    expect(badgeLook({ kind: "Terminal", harness: "Shell" })).toEqual({ icon: "computer-terminal-01", fill: "neutral" });
    expect(badgeLook({ kind: "File", harness: null })).toEqual({ icon: "file-01", fill: "bare" });
  });

  it("dots waiting and done tabs and marks chats and cloud tabs", () => {
    expect(stateDot("WaitingForInput")).toBe("waiting");
    expect(stateDot("Working")).toBeUndefined();
    expect(tabMarks({ kind: "Chat", cloud: null })).toEqual(["bubble-chat"]);
    expect(tabMarks({ kind: "Terminal", cloud: { runner: 1 } as never })).toEqual(["cloud"]);
  });
});
