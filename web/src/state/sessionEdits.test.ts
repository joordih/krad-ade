import { describe, expect, it } from "vitest";
import {
  chatNode,
  closeNodeCommands,
  closeSessionCommands,
  newSessionCommand,
  nextSessionId,
  nodeTitle,
  openChatCommand,
  openNodeCommand,
  openTerminalCommand,
  renameNodeCommand,
  renameSessionCommand,
  terminalNode,
} from "./sessionEdits";

const chat = { environment: "main", terminal: 7 };

describe("session edits", () => {
  it("takes the id after the highest session", () => {
    expect(nextSessionId(undefined)).toBe(1);
    expect(nextSessionId([])).toBe(1);
    expect(nextSessionId([{ id: 1 }, { id: 4 }, { id: 2 }])).toBe(5);
  });

  it("names a tab after its label and number", () => {
    expect(nodeTitle("New chat", 7)).toBe("New chat · 7");
  });

  it("opens a session with the default name in its group", () => {
    expect(newSessionCommand("main", 5, 2)).toEqual({
      area: "session",
      environment: "main",
      edit: { verb: "open", session: 5, group: 2, name: "", shown: true },
    });
    expect(newSessionCommand("main", 1, null).edit).toMatchObject({ group: null, shown: true });
  });

  it("renames only to a non-empty name", () => {
    expect(renameSessionCommand("main", 3, "  Refunds ")).toEqual({
      area: "session",
      environment: "main",
      edit: { verb: "rename", session: 3, name: "Refunds" },
    });
    expect(renameSessionCommand("main", 3, "   ")).toBeUndefined();
    expect(renameNodeCommand("main", 7, "")).toBeUndefined();
    expect(renameNodeCommand("main", 7, " Build ")).toEqual({
      area: "node",
      environment: "main",
      edit: { verb: "set", terminal: 7, field: { field: "title", value: "Build" } },
    });
  });

  it("stops a tab's terminal before dropping its node", () => {
    expect(closeNodeCommands("main", 7).map((command) => command.area)).toEqual(["terminal", "node"]);
  });

  it("closes every terminal and node of a session, then the session", () => {
    const commands = closeSessionCommands("main", 3, [7, 8]);
    expect(commands.map((command) => command.area)).toEqual([
      "terminal",
      "node",
      "terminal",
      "node",
      "session",
    ]);
    expect(commands.at(-1)).toEqual({
      area: "session",
      environment: "main",
      edit: { verb: "close", session: 3 },
    });
  });

  it("builds nodes the runtime can place as a tab", () => {
    const node = chatNode(7, 2, "Codex", "New chat · 7");
    expect(node).toEqual({
      id: 7,
      title: "New chat · 7",
      state: "Idle",
      harness: "Codex",
      group: 2,
      text_scale: 1,
      kind: "Chat",
    });
    expect(openNodeCommand("main", node)).toEqual({
      area: "node",
      environment: "main",
      edit: { verb: "open", node, place: { at: "tab", anchor: null } },
    });
    expect(openTerminalCommand("main", 8, "Shell", "/repo")).toEqual({
      area: "terminal",
      verb: "open",
      terminal: { environment: "main", terminal: 8 },
      intent: { intent: "preset", preset: "Shell" },
      cwd: "/repo",
    });
    expect(terminalNode(8, null, "Shell · 8").harness).toBeUndefined();
  });
});

describe("opening a chat", () => {
  it("resumes the node's session when the agent reported one", () => {
    const node = { ...chatNode(7, 2, "Codex", "chat"), resume: "4f2c0b7c-a91e" };
    const command = openChatCommand(chat, node, () => "new");
    expect(command.verb).toBe("open");
    if (command.verb !== "open") return;
    expect(command.start.session).toEqual({ kind: "resume", from: "4f2c0b7c-a91e" });
    expect(command.start.harness).toBe("Codex");
    expect(command.chat).toEqual(chat);
  });

  it("starts a fresh session and falls back to Claude Code", () => {
    const node = { ...terminalNode(7, 2, "tab"), harness: "Shell" as const };
    const command = openChatCommand(chat, node, () => "made-up");
    if (command.verb !== "open") throw new Error("expected an open command");
    expect(command.start.session).toEqual({ kind: "start", from: "made-up" });
    expect(command.start.harness).toBe("ClaudeCode");
    expect(command.start.acp).toEqual({ mode: null, options: [] });
    expect(command.start.messages).toEqual([]);
  });
});
