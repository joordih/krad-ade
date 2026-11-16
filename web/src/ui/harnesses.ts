import type { AcpAgent } from "../protocol/AcpAgent";
import type { AgentPreset } from "../protocol/AgentPreset";

/** The slice of the runtime settings the harness menus read. */
export type HarnessSource = {
  default_harness?: AgentPreset;
  acp?: { agents?: readonly AcpAgent[] } | null;
};

export type HarnessChoice = {
  preset: AgentPreset;
  label: string;
  icon: string;
};

export const DEFAULT_CHAT_HARNESS: AgentPreset = "ClaudeCode";

export const NATIVE_HARNESSES: readonly AgentPreset[] = [
  "Shell",
  "ClaudeCode",
  "Codex",
  "Opencode",
  "Gateway",
];

const ACP_PREFIX = "Acp:";

const NATIVE_LABEL: Record<string, string> = {
  Shell: "Shell",
  ClaudeCode: "Claude Code",
  Codex: "Codex",
  Opencode: "opencode",
  Gateway: "Gateway",
};

const NATIVE_ICON: Record<string, string> = {
  Shell: "computer-terminal-01",
  ClaudeCode: "brand-claude",
  Codex: "brand-openai",
  Opencode: "brand-opencode",
  Gateway: "cloud",
};

export function isChatHarness(preset: AgentPreset): boolean {
  return preset !== "Shell";
}

export function harnessIcon(preset: AgentPreset): string {
  return NATIVE_ICON[preset] ?? "plug-socket";
}

function acpId(preset: AgentPreset): string | undefined {
  return preset.startsWith(ACP_PREFIX) ? preset.slice(ACP_PREFIX.length) : undefined;
}

/** The name the settings give a harness, else the desktop's display name. */
export function harnessLabel(settings: HarnessSource | undefined, preset: AgentPreset): string {
  const id = acpId(preset);
  if (!id) return NATIVE_LABEL[preset] ?? preset;
  const custom = settings?.acp?.agents?.find((agent) => agent.id === id);
  return custom?.name ?? id;
}

function acpPresets(settings: HarnessSource | undefined): AgentPreset[] {
  return (settings?.acp?.agents ?? []).map((agent) => `Acp:${agent.id}` as AgentPreset);
}

/** Every harness the runtime can start a terminal with, native ones first. */
export function harnessChoices(settings: HarnessSource | undefined): HarnessChoice[] {
  return [...NATIVE_HARNESSES, ...acpPresets(settings)].map((preset) => ({
    preset,
    label: harnessLabel(settings, preset),
    icon: harnessIcon(preset),
  }));
}

/** The harnesses that open a chat rather than a plain terminal. */
export function chatHarnessChoices(settings: HarnessSource | undefined): HarnessChoice[] {
  return harnessChoices(settings).filter((choice) => isChatHarness(choice.preset));
}

/** The harness a new chat opens with: the setting when it can chat, else Claude Code. */
export function defaultChatHarness(settings: HarnessSource | undefined): AgentPreset {
  const chosen = settings?.default_harness;
  return chosen && isChatHarness(chosen) ? chosen : DEFAULT_CHAT_HARNESS;
}
