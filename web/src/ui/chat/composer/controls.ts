import type { Effort } from "../../../protocol/Effort";
import type { PermissionMode } from "../../../protocol/PermissionMode";

export const PERMISSION_MODES: readonly PermissionMode[] = ["manual", "acceptEdits", "plan", "auto"];

const PERMISSION_LABEL: Record<PermissionMode, string> = {
  manual: "Ask before edits",
  acceptEdits: "Accept edits",
  plan: "Plan",
  auto: "Auto",
  dontAsk: "Don't ask",
  bypassPermissions: "Bypass permissions",
};

const EFFORT_LABEL: Record<Effort, string> = {
  minimal: "Minimal",
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra high",
  max: "Max",
  ultra: "Ultra",
};

const EFFORT_MEANING: Record<Effort, string> = {
  minimal: "Answers almost at once, with barely any reasoning",
  low: "Quick answers with light reasoning",
  medium: "Balances speed and depth",
  high: "Reasons longer before answering",
  xhigh: "Deep reasoning for hard problems",
  max: "The most reasoning the model allows per turn",
  ultra: "The longest reasoning, for the hardest tasks",
};

const DEFAULT_EFFORT_LABEL = "Default";
const DEFAULT_MODE: PermissionMode = "manual";
const WINDOW_MILLION = 1_000_000;
const WINDOW_THOUSAND = 1_000;
const CONTEXT_WARN_PERCENT = 80;

export type EffortTrigger = "hidden" | "disabled" | "enabled";
export type SendState = "stop" | "idle" | "ready";

export function permissionLabel(mode: PermissionMode | null | undefined): string {
  return PERMISSION_LABEL[mode ?? DEFAULT_MODE];
}

export function effortLabel(effort: Effort | null | undefined): string {
  return effort ? EFFORT_LABEL[effort] : DEFAULT_EFFORT_LABEL;
}

export function effortMeaning(effort: Effort): string {
  return EFFORT_MEANING[effort];
}

export function effortTrigger(offersEffort: boolean, steps: number): EffortTrigger {
  if (!offersEffort) return "hidden";
  return steps > 1 ? "enabled" : "disabled";
}

export function levelShare(step: number, steps: number): number {
  if (steps <= 0) return 0;
  if (steps === 1) return 1;
  return Math.min(step, steps - 1) / (steps - 1);
}

export function meterStep(efforts: readonly Effort[], effort: Effort | null, fallback: Effort | null): number | undefined {
  const shown = effort ?? fallback;
  const at = shown ? efforts.indexOf(shown) : -1;
  return at >= 0 ? at : undefined;
}

export function sendState(empty: boolean, working: boolean): SendState {
  if (empty) return working ? "stop" : "idle";
  return "ready";
}

export function windowLabel(tokens: number): string {
  if (tokens >= WINDOW_MILLION && tokens % WINDOW_MILLION === 0) return `${tokens / WINDOW_MILLION}M`;
  if (tokens >= WINDOW_THOUSAND) return `${Math.floor(tokens / WINDOW_THOUSAND)}K`;
  return String(tokens);
}

export function contextWarns(percent: number): boolean {
  return percent >= CONTEXT_WARN_PERCENT;
}
