import { describe, expect, it } from "vitest";
import type { HostsSnapshot } from "../protocol/HostsSnapshot";
import {
  chatHarnesses,
  defaultChatHarness,
  detectedAgents,
  DEFAULT_CHAT_HARNESS,
  harnessChoices,
  harnessIcon,
  harnessLabel,
  switchableHarnesses,
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

  it("lists the chat harnesses in the desktop's chat order, without the shell", () => {
    expect(chatHarnesses(settings)).toEqual(["ClaudeCode", "Opencode", "Codex", "Gateway", "Acp:my-agent"]);
  });

  it("reads the agents found on this computer or on the environment's SSH host", () => {
    const hosts = {
      hosts: [{ environment: "remote", agents: ["Codex"] }],
      prompts: [],
      local: [
        { place: { place: "wsl", distro: "Ubuntu" }, agents: ["Opencode"] },
        { place: { place: "native" }, agents: ["ClaudeCode", "Codex"] },
      ],
      adapters: [],
    } as unknown as HostsSnapshot;
    expect(detectedAgents(hosts, "work", { kind: "local" })).toEqual(["ClaudeCode", "Codex"]);
    expect(detectedAgents(hosts, "remote", { kind: "ssh", destination: "box" } as never)).toEqual(["Codex"]);
    expect(detectedAgents(undefined, "work", undefined)).toEqual([]);
  });

  it("offers a fresh chat the other harnesses found here, and the Gateway only when ready and not in the cloud", () => {
    const ready = { state: "ready", place: { place: "native" }, models: [] } as never;
    const place = { current: "ClaudeCode" as const, detected: ["ClaudeCode", "Codex", "Acp:my-agent"] as const, gateway: ready, inCloud: false };
    expect(switchableHarnesses(settings, place)).toEqual(["Codex", "Gateway", "Acp:my-agent"]);
    expect(switchableHarnesses(settings, { ...place, inCloud: true })).toEqual(["Codex", "Acp:my-agent"]);
    expect(switchableHarnesses(settings, { ...place, gateway: { state: "checking" } })).toEqual(["Codex", "Acp:my-agent"]);
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
