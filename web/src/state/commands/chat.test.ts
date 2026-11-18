import { describe, expect, it } from "vitest";
import { mentionsCommand, pickModelCommands, promptCommand } from "./chat";

const chat = { environment: "main", terminal: 3 };

describe("chat commands", () => {
  it("sends a trimmed prompt, queues it while the agent works and skips an empty one", () => {
    expect(promptCommand(chat, "  hi  ", [], false)).toEqual({ area: "chat", verb: "send", chat, prompt: { text: "hi", mentions: [] } });
    expect(promptCommand(chat, "hi", [], true)?.verb).toBe("queue");
    expect(promptCommand(chat, "   ", [], false)).toBeUndefined();
    expect(promptCommand(chat, "", [{ Path: "/r/a.png" }], false)?.verb).toBe("send");
  });

  it("asks for mentions under the group's folder", () => {
    expect(mentionsCommand("main", "/r", "src")).toEqual({
      area: "chat",
      verb: "mentions",
      environment: "main",
      root: "/r",
      query: "src",
      limit: 200,
    });
  });

  it("sets the model of the chat's own harness, and moves a fresh chat to another harness first", () => {
    const resume = () => "r-1";
    expect(pickModelCommands(chat, "ClaudeCode", "ClaudeCode", "opus", resume)).toEqual([
      { area: "chat", verb: "set_model", chat, model: "opus" },
    ]);
    const moved = pickModelCommands(chat, "ClaudeCode", "Codex", "gpt", resume);
    expect(moved.map((command) => command.area)).toEqual(["chat", "node", "chat", "chat"]);
    expect(moved[0]).toEqual({ area: "chat", verb: "close", chat });
    expect(moved[3]).toEqual({ area: "chat", verb: "set_model", chat, model: "gpt" });
    expect(moved[1]).toEqual({ area: "node", environment: "main", edit: { verb: "set", terminal: 3, field: { field: "harness", value: "Codex" } } });
    expect(moved[2]).toMatchObject({ area: "chat", verb: "open", chat, start: { harness: "Codex", session: { kind: "start", from: "r-1" } } });
  });
});
