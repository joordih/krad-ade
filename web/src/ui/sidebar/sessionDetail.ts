import type { AgentPreset } from "../../protocol/AgentPreset";
import type { AgentState } from "../../protocol/AgentState";
import type { SessionIcon } from "../../protocol/SessionIcon";
import type { TerminalNode } from "../../protocol/TerminalNode";
import { harnessLabel, type HarnessSource } from "../../state/harnesses";
import type { IconTone } from "../primitives/Icon";

export type StatusLook = { icon: string; tone: IconTone; spins: boolean };

const SESSION_ICON: Record<SessionIcon, string> = {
  folder: "folder-01",
  terminal: "computer-terminal-01",
  book: "book-open-01",
  note: "note-01",
  flag: "flag-02",
  branch: "git-branch",
  layers: "layers-01",
  home: "home-01",
  spark: "ai-magic",
  chat: "bubble-chat",
  tasks: "task-done-01",
  diamond: "diamond-01",
  plug: "plug-socket",
  people: "user-group",
  flow: "flow-connection",
  target: "target-02",
};

const STATUS_LOOK: Record<AgentState, StatusLook | undefined> = {
  Idle: undefined,
  Working: { icon: "loading-03", tone: "muted", spins: true },
  WaitingForInput: { icon: "radio-button", tone: "attention", spins: false },
  Done: { icon: "checkmark-circle-02", tone: "strong", spins: false },
};

const DEFAULT_SESSION_ICON: SessionIcon = "layers";
const CAPTION_SEPARATOR = " · ";
const TABS_SUFFIX = "tabs";
const SHELL: AgentPreset = "Shell";

export const NEEDS_YOU = "needs you";

export function sessionIcon(icon: SessionIcon | null | undefined): string {
  return SESSION_ICON[icon ?? DEFAULT_SESSION_ICON];
}

export function statusLook(state: AgentState): StatusLook | undefined {
  return STATUS_LOOK[state];
}

export function sessionHarness(nodes: readonly TerminalNode[]): AgentPreset | undefined {
  return nodes.map((node) => node.harness).find((harness): harness is AgentPreset => harness != null && harness !== SHELL);
}

export function sessionCaption(nodes: readonly TerminalNode[], settings: HarnessSource | undefined): string {
  const harness = sessionHarness(nodes);
  const tabs = `${nodes.length} ${TABS_SUFFIX}`;
  return harness ? [harnessLabel(settings, harness), tabs].join(CAPTION_SEPARATOR) : tabs;
}
