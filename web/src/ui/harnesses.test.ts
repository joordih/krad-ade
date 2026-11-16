import { describe, expect, it } from "vitest";
import {
  chatHarnessChoices,
  defaultChatHarness,
  DEFAULT_CHAT_HARNESS,
  harnessChoices,
  harnessIcon,
  harnessLabel,
  type HarnessSource,
} from "./harnesses";

const settings: HarnessSource = {
  default_harness: "Codex",
  acp: { agents: [{ id: "my-agent", name: "My Agent", program: "my-agent" }] },
};

describe("harness choices", () => {
  it("lists the native harnesses and the configured ACP agents", () => {
    const choices = harnessChoices(settings);
    expect(choices.map((choice) => choice.label)).toEqual([
      "Shell",
      "Claude Code",
      "Codex",
      "opencode",
      "Gateway",
      "My Agent",
    ]);
    expect(choices.at(-1)?.preset).toBe("Acp:my-agent");
    expect(choices.at(-1)?.icon).toBe("plug-socket");
  });

  it("offers only chat harnesses for a new chat", () => {
    const labels = chatHarnessChoices(settings).map((choice) => choice.label);
    expect(labels).not.toContain("Shell");
    expect(labels).toContain("Claude Code");
    expect(labels).toContain("My Agent");
  });

  it("uses the default harness when it can chat and falls back otherwise", () => {
    expect(defaultChatHarness(settings)).toBe("Codex");
    expect(defaultChatHarness({ default_harness: "Shell" })).toBe(DEFAULT_CHAT_HARNESS);
    expect(defaultChatHarness(undefined)).toBe(DEFAULT_CHAT_HARNESS);
  });

  it("names and draws a harness from its settings", () => {
    expect(harnessLabel(settings, "ClaudeCode")).toBe("Claude Code");
    expect(harnessLabel(settings, "Acp:my-agent")).toBe("My Agent");
    expect(harnessLabel(undefined, "Acp:gone")).toBe("gone");
    expect(harnessIcon("Codex")).toBe("brand-openai");
  });
});
