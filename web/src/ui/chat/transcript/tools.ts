import type { AnswerPart } from "../../../protocol/AnswerPart";
import type { ToolCall } from "../../../protocol/ToolCall";
import type { ToolKind } from "../../../protocol/ToolKind";

export type ToolLook = "shell" | "read" | "edit" | "search" | "agent" | "plan" | "web" | "other";

type Wording = { row: string; summary: string; one: string; many: string; doing: string };

export type Block = { kind: "part"; part: Exclude<AnswerPart, { tool: ToolCall }>; at: number } | { kind: "tools"; calls: ToolCall[]; at: number };

const LOOK_OF: Record<ToolKind, ToolLook> = {
  Shell: "shell",
  Read: "read",
  Edit: "edit",
  Search: "search",
  Subagent: "agent",
  Checklist: "plan",
  Fetch: "web",
  Question: "other",
  Plan: "other",
  Other: "other",
};

const ICON_OF: Record<ToolLook, string> = {
  shell: "computer-terminal-01",
  read: "file-01",
  edit: "file-01",
  search: "search-01",
  agent: "workflow-square-10",
  plan: "task-done-01",
  web: "square-arrow-up-right",
  other: "plug-socket",
};

const WORDING: Record<ToolLook, Wording> = {
  shell: { row: "Ran", summary: "Ran", one: "command", many: "commands", doing: "Running commands" },
  read: { row: "Read", summary: "Read", one: "file", many: "files", doing: "Reading files" },
  edit: { row: "Edited", summary: "Edited", one: "file", many: "files", doing: "Editing files" },
  search: { row: "Searched", summary: "Ran", one: "search", many: "searches", doing: "Searching" },
  agent: { row: "Started", summary: "Started", one: "agent", many: "agents", doing: "Starting an agent" },
  plan: { row: "Updated", summary: "Updated", one: "plan", many: "plans", doing: "Planning" },
  web: { row: "Fetched", summary: "Fetched", one: "page", many: "pages", doing: "Fetching the web" },
  other: { row: "Used", summary: "Used", one: "tool", many: "tools", doing: "Using tools" },
};

const SUMMARY_SEPARATOR = ", ";
const CUT_MARK = "…";

export function lookOf(call: Pick<ToolCall, "kind">): ToolLook {
  return LOOK_OF[call.kind];
}

export function toolIcon(look: ToolLook): string {
  return ICON_OF[look];
}

export function namesFile(look: ToolLook): boolean {
  return look === "read" || look === "edit";
}

export function rowVerb(call: ToolCall): string {
  const look = lookOf(call);
  return look === "other" ? call.name : WORDING[look].row;
}

export function doingLabel(look: ToolLook): string {
  return WORDING[look].doing;
}

export function doneSummary(calls: readonly ToolCall[]): string {
  const counts = new Map<ToolLook, number>();
  for (const call of calls) counts.set(lookOf(call), (counts.get(lookOf(call)) ?? 0) + 1);
  const summary = [...counts]
    .map(([look, count]) => {
      const wording = WORDING[look];
      return `${wording.summary.toLowerCase()} ${count} ${count === 1 ? wording.one : wording.many}`;
    })
    .join(SUMMARY_SEPARATOR);
  return summary.charAt(0).toUpperCase() + summary.slice(1);
}

export function oneLine(summary: string): string {
  const [first = "", ...rest] = summary.trim().split("\n");
  return rest.length > 0 ? `${first.trimEnd()}${CUT_MARK}` : first;
}

export function blocksOf(parts: readonly AnswerPart[]): Block[] {
  const blocks: Block[] = [];
  parts.forEach((part, at) => {
    if (!("tool" in part)) {
      blocks.push({ kind: "part", part, at });
      return;
    }
    const last = blocks.at(-1);
    if (last?.kind === "tools") last.calls.push(part.tool);
    else blocks.push({ kind: "tools", calls: [part.tool], at });
  });
  return blocks;
}
