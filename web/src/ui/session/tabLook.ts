import type { AgentState } from "../../protocol/AgentState";
import type { TerminalNode } from "../../protocol/TerminalNode";
import { harnessIcon, harnessTone, type HarnessTone } from "../../state/harnesses";

export type BadgeLook = { icon: string; fill: HarnessTone | "neutral" | "bare" };
export type StateDot = "waiting" | "done";

const FILE_ICON = "file-01";
const CHAT_MARK = "bubble-chat";
const CLOUD_MARK = "cloud";

const STATE_DOT: Record<AgentState, StateDot | undefined> = {
  WaitingForInput: "waiting",
  Done: "done",
  Idle: undefined,
  Working: undefined,
};

export function badgeLook(node: Pick<TerminalNode, "kind" | "harness">): BadgeLook {
  if (node.kind === "File") return { icon: FILE_ICON, fill: "bare" };
  const harness = node.harness ?? "Shell";
  return { icon: harnessIcon(harness), fill: harness === "Shell" ? "neutral" : harnessTone(harness) };
}

export function stateDot(state: AgentState): StateDot | undefined {
  return STATE_DOT[state];
}

export function tabMarks(node: Pick<TerminalNode, "kind" | "cloud">): string[] {
  const marks: string[] = [];
  if (node.kind === "Chat") marks.push(CHAT_MARK);
  if (node.cloud) marks.push(CLOUD_MARK);
  return marks;
}
