import { describe, expect, it } from "vitest";
import type { TerminalNode } from "../../protocol/TerminalNode";
import { sessionCaption, sessionIcon, statusLook } from "./sessionDetail";

function node(id: number, harness: TerminalNode["harness"]): TerminalNode {
  return { id, title: `tab ${id}`, state: "Idle", text_scale: 1, kind: "Chat", harness };
}

describe("a session row", () => {
  it("captions the first agent harness and the tab count like the desktop", () => {
    expect(sessionCaption([node(1, "Shell"), node(2, "ClaudeCode")], undefined)).toBe("Claude Code · 2 tabs");
    expect(sessionCaption([node(1, "Shell")], undefined)).toBe("1 tabs");
  });

  it("draws the session's own icon, layers by default", () => {
    expect(sessionIcon(undefined)).toBe("layers-01");
    expect(sessionIcon("branch")).toBe("git-branch");
  });

  it("shows a status only for working, waiting and done sessions", () => {
    expect(statusLook("Idle")).toBeUndefined();
    expect(statusLook("Working")?.spins).toBe(true);
    expect(statusLook("WaitingForInput")?.tone).toBe("attention");
  });
});
