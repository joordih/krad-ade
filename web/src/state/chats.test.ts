import { describe, expect, it } from "vitest";
import type { ChatMessage } from "../protocol/ChatMessage";
import type { ChatState } from "../protocol/ChatState";
import { harnessSwitchable } from "./chats";

const idle = { working: false, earlier: false } as ChatState;

describe("switching a chat's harness", () => {
  it("is allowed only on a fresh, idle chat of this computer's runtime", () => {
    const fresh = { state: idle, from: 0, messages: [] };
    expect(harnessSwitchable("local", fresh)).toBe(true);
    expect(harnessSwitchable("runner:1", fresh)).toBe(false);
    expect(harnessSwitchable("local", { ...fresh, messages: [{} as ChatMessage] })).toBe(false);
    expect(harnessSwitchable("local", { ...fresh, from: 4 })).toBe(false);
    expect(harnessSwitchable("local", { ...fresh, state: { ...idle, working: true } })).toBe(false);
    expect(harnessSwitchable("local", { ...fresh, state: { ...idle, earlier: true } })).toBe(false);
  });
});
