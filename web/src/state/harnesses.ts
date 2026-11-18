import type { AcpAgent } from "../protocol/AcpAgent";
import type { AgentPreset } from "../protocol/AgentPreset";
import type { GatewayState } from "../protocol/GatewayState";
import type { Host } from "../protocol/Host";
import type { HostsSnapshot } from "../protocol/HostsSnapshot";

export type HarnessSource = {
  default_harness?: AgentPreset;
  acp?: { agents?: readonly AcpAgent[] } | null;
};

export type HarnessTone = "brand" | "strong" | "accent" | "subtle";

type NativeHarness = Exclude<AgentPreset, `Acp:${string}`>;

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

const CHAT_ORDER: readonly AgentPreset[] = ["ClaudeCode", "Opencode", "Codex", "Gateway"];

const ACP_PREFIX = "Acp:" as const;

const NATIVE_LABEL: Record<NativeHarness, string> = {
  Shell: "Shell",
  ClaudeCode: "Claude Code",
  Codex: "Codex",
  Opencode: "opencode",
  Gateway: "Gateway",
};

const NATIVE_ICON: Record<NativeHarness, string> = {
  Shell: "computer-terminal-01",
  ClaudeCode: "brand-claude",
  Codex: "brand-openai",
  Opencode: "brand-opencode",
  Gateway: "flow-connection",
};

const NATIVE_TONE: Record<NativeHarness, HarnessTone> = {
  Shell: "subtle",
  ClaudeCode: "brand",
  Codex: "strong",
  Opencode: "strong",
  Gateway: "accent",
};

const ACP_ICON = "plug-socket";
const ACP_TONE: HarnessTone = "strong";

function isNative(preset: AgentPreset): preset is NativeHarness {
  return !preset.startsWith(ACP_PREFIX);
}

export function isChatHarness(preset: AgentPreset): boolean {
  return preset !== "Shell";
}

export function harnessIcon(preset: AgentPreset): string {
  return isNative(preset) ? NATIVE_ICON[preset] : ACP_ICON;
}

export function harnessTone(preset: AgentPreset): HarnessTone {
  return isNative(preset) ? NATIVE_TONE[preset] : ACP_TONE;
}

function acpId(preset: AgentPreset): string | undefined {
  return preset.startsWith(ACP_PREFIX) ? preset.slice(ACP_PREFIX.length) : undefined;
}

export function harnessLabel(settings: HarnessSource | undefined, preset: AgentPreset): string {
  const id = acpId(preset);
  if (!id) return isNative(preset) ? NATIVE_LABEL[preset] : preset;
  const custom = settings?.acp?.agents?.find((agent) => agent.id === id);
  return custom?.name ?? id;
}

function acpPresets(settings: HarnessSource | undefined): AgentPreset[] {
  return (settings?.acp?.agents ?? []).map((agent): AgentPreset => `${ACP_PREFIX}${agent.id}`);
}

export function harnessChoices(settings: HarnessSource | undefined): HarnessChoice[] {
  return [...NATIVE_HARNESSES, ...acpPresets(settings)].map((preset) => ({
    preset,
    label: harnessLabel(settings, preset),
    icon: harnessIcon(preset),
  }));
}

export function chatHarnesses(settings: HarnessSource | undefined): AgentPreset[] {
  return [...CHAT_ORDER, ...acpPresets(settings)];
}

export function detectedAgents(hosts: HostsSnapshot | undefined, environment: string, host: Host | undefined): readonly AgentPreset[] {
  if (!hosts) return [];
  if (host && host.kind !== "local") return hosts.hosts.find((known) => known.environment === environment)?.agents ?? [];
  return hosts.local.find((known) => known.place.place === "native")?.agents ?? [];
}

export type SwitchPlace = {
  current: AgentPreset;
  detected: readonly AgentPreset[];
  gateway: GatewayState | undefined;
  inCloud: boolean;
};

export function switchableHarnesses(settings: HarnessSource | undefined, place: SwitchPlace): AgentPreset[] {
  return chatHarnesses(settings).filter((other) => {
    if (other === place.current) return false;
    if (other === "Gateway") return place.gateway?.state === "ready" && !place.inCloud;
    return place.detected.includes(other);
  });
}

export function defaultChatHarness(settings: HarnessSource | undefined): AgentPreset {
  const chosen = settings?.default_harness;
  return chosen && isChatHarness(chosen) ? chosen : DEFAULT_CHAT_HARNESS;
}
